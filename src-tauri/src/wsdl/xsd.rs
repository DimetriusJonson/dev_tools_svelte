use std::collections::HashMap;

use crate::wsdl::parser::{get_attr, get_reference, is_xsd_ns, XmlDom};
use crate::wsdl::{MAX_TEMPLATE_DEPTH, MAX_TEMPLATE_ELEMENTS, XSD_NAMESPACE};

/// Separator inside definition keys. It cannot appear in a namespace URI or in an
/// XSD name, so a key splits back into its two parts without ambiguity.
const KEY_SEPARATOR: char = '\u{0001}';

/// `maxOccurs="unbounded"` is kept as zero, which means "no cap is implied".
const UNBOUNDED: u8 = 0;

/// A repeating element is shown this many times before the comment that tells
/// the user to copy the block.
const REPETITIONS: usize = 2;

/// Prefix used for a namespace the schema never declared. Printing the URI as a
/// prefix would be unreadable, and printing nothing silently changes meaning.
const GENERATED_PREFIX: &str = "ns";

fn key(uri: &str, local: &str) -> String {
    format!("{uri}{KEY_SEPARATOR}{local}")
}

/// Charges one node to the element budget. `false` means the budget is spent and
/// the walk stops, which is what keeps a hostile schema from building a
/// megabyte-long template.
fn budget_checked(depth: u8, budget: &mut i32, warnings: &mut Vec<String>) -> bool {
    if *budget <= 0 {
        if !warnings.contains(&"element-limit".to_string()) {
            warnings.push("element-limit".to_string());
        }
        return false;
    }
    *budget -= 1 + depth as i32;
    true
}

/// Finds a global declaration by namespace URI and local name. An unprefixed
/// QName names no namespace by the specification, and that is how a single-schema
/// WSDL usually spells a reference to its own types, so a name that exists in one
/// of the declared target namespaces is accepted too.
fn lookup<'a>(
    map: &'a HashMap<String, Declaration>,
    by_local: &HashMap<String, Vec<(String, usize)>>,
    uri: &str,
    local: &str,
) -> Option<&'a Declaration> {
    let exact = map.get(&key(uri, local));
    if exact.is_some() || !uri.is_empty() {
        return exact;
    }
    for (namespace, _) in by_local.get(local)?.iter() {
        if let Some(found) = map.get(&key(namespace, local)) {
            return Some(found);
        }
    }
    None
}

/// One entry of a template: a child element or an attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    uri: String,
    local: String,
    /// Declaring type name, used as the comment text when the value is unknown.
    type_name: String,
    kind: Kind,
    occurs: Occurs,
    /// `true` for attributes, which print inline on the parent tag.
    attribute: bool,
    nillable: bool,
}

/// What a resolved declaration contributes to the template.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    /// Text content only.
    Leaf { sample: String },
    /// Nested markup.
    Composite { fields: Vec<Field> },
    /// Alternatives: every branch is printed, commented out, so the user picks one.
    Choice { branches: Vec<Field> },
    /// A type that could not be resolved, or a cycle: a comment names it.
    Unknown,
}

/// A resolved type or element declaration. `type_name` is the declaration the
/// walk landed on: it becomes the comment text when a value cannot be guessed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Definition {
    type_name: String,
    kind: Kind,
    /// True when the declaration can carry text, which is how a type with simple
    /// content is recognized.
    has_text: bool,
}

impl Definition {
    fn unknown(type_name: &str) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Unknown,
            has_text: false,
        }
    }

    fn leaf(type_name: &str, sample: &str) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Leaf {
                sample: sample.to_string(),
            },
            has_text: true,
        }
    }

    fn composite(type_name: &str, fields: Vec<Field>) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Composite { fields },
            has_text: false,
        }
    }
}

/// What a local declaration of a schema means namespace-wise. `elementFormDefault`
/// decides it for elements and `attributeFormDefault` for attributes, and both
/// default to `unqualified` — the spelling almost every WSDL uses.
#[derive(Clone, Debug, Default)]
struct Context {
    target_ns: String,
    element_form: String,
    attribute_form: String,
}

impl Context {
    fn of(dom: &XmlDom, schema: usize) -> Context {
        Context {
            target_ns: get_attr(dom, schema, "targetNamespace").unwrap_or_default(),
            element_form: get_attr(dom, schema, "elementFormDefault").unwrap_or_default(),
            attribute_form: get_attr(dom, schema, "attributeFormDefault").unwrap_or_default(),
        }
    }

    /// Namespace a local piece of markup belongs to at a given nesting level.
    /// The declaration itself is always in the target namespace, whatever the
    /// form says — the form only affects the children it contains.
    fn namespace_for(&self, local: &str, depth: u8) -> String {
        let qualified = depth > 0 && local == "qualified";
        if qualified {
            self.target_ns.clone()
        } else {
            String::new()
        }
    }
}

/// How many times a piece of markup appears in an instance document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Occurs {
    min: u8,
    max: u8,
}

impl Default for Occurs {
    fn default() -> Self {
        Occurs { min: 1, max: 1 }
    }
}

fn positive(text: &str) -> Option<u8> {
    text.trim().parse::<u8>().ok()
}

/// Reads `minOccurs`/`maxOccurs`. A non-numeric value is treated as the default
/// rather than as a schema error: the goal is a usable template, not validation.
fn occurs(dom: &XmlDom, index: usize) -> Occurs {
    let mut result = Occurs::default();
    if let Some(text) = get_attr(dom, index, "minOccurs") {
        if text.trim() == "unbounded" {
            result.min = UNBOUNDED;
        } else if let Some(value) = positive(&text) {
            result.min = value;
        }
    }
    if let Some(text) = get_attr(dom, index, "maxOccurs") {
        if text.trim() == "unbounded" {
            result.max = UNBOUNDED;
        } else if let Some(value) = positive(&text) {
            result.max = value;
        }
    }
    if result.max == UNBOUNDED || result.max >= result.min {
        return result;
    }
    Occurs {
        min: result.min,
        max: result.min,
    }
}

/// The value a built-in type written in the schema namespace produces. A name
/// in another namespace is never a builtin, however it is spelled: `int` in the
/// target namespace is a user type that shadows the built-in one.
fn builtin_leaf(uri: &str, local: &str) -> Option<Leaf> {
    if !is_xsd_ns(uri) {
        return None;
    }
    let sample = builtin_sample(local)?;
    Some(Leaf {
        type_name: local.to_string(),
        sample,
    })
}

/// A leaf value: the element's own type, or the item type of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Leaf {
    /// `xsd:string` and friends, or the name of a user type when nothing builtin
    /// could be reached — that name becomes the comment text.
    type_name: String,
    sample: String,
}

/// The built-in types a template can fill with plausible text. Anything else is
/// reported as a comment instead of guessed at.
fn builtin_sample(type_name: &str) -> Option<String> {
    let sample = match type_name {
        "string" | "ID" | "IDREF" | "IDREFS" | "NCName" | "NMTOKEN" | "NMTOKENS" | "QName" => {
            "text"
        }
        "normalizedString" | "token" => "text",
        "base64Binary" | "hexBinary" => "",
        "boolean" => "false",
        "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
        | "positiveInteger" | "nonPositiveInteger" | "negativeInteger" | "unsignedInt"
        | "unsignedLong" | "unsignedShort" | "unsignedByte" => "0",
        "decimal" | "float" | "double" => "0",
        "date" => "2026-01-01",
        "time" => "12:00:00",
        "dateTime" => "2026-01-01T12:00:00",
        "duration" => "PT0S",
        "gYear" => "2026",
        "gMonth" => "--01",
        "gDay" => "---01",
        "language" => "en",
        _ => return None,
    };
    Some(sample.to_string())
}

/// Everything one template walk needs: where definitions are registered, the
/// recursion guard, the element budget and the warnings collected so far.
struct Walk<'a> {
    dom: &'a XmlDom,
    index: &'a XsdIndex,
    /// Definitions being expanded. A name already in here is a cycle, which is
    /// reported instead of following the reference forever.
    active: Vec<String>,
    budget: i32,
    warnings: Vec<String>,
}

impl<'a> Walk<'a> {
    fn new(dom: &'a XmlDom, index: &'a XsdIndex) -> Walk<'a> {
        Walk {
            dom,
            index,
            active: Vec::new(),
            budget: MAX_TEMPLATE_ELEMENTS as i32,
            warnings: Vec::new(),
        }
    }

    /// Records a warning once; the UI turns the code into localized text.
    fn note(&mut self, code: &str) {
        if !self.warnings.iter().any(|known| known == code) {
            self.warnings.push(code.to_string());
        }
    }

    /// True while the walk may still produce markup.
    fn charge(&mut self, depth: u8) -> bool {
        budget_checked(depth, &mut self.budget, &mut self.warnings)
    }

    /// Resolves a QName written on `index` and reports whether it names a
    /// built-in type. A prefix bound to the schema namespace is what makes a
    /// builtin reachable without a declaration.
    fn builtin(&mut self, index: usize, attribute: &str, depth: u8) -> Option<Leaf> {
        let (uri, local) = get_reference(self.dom, index, attribute)?;
        if let Some(leaf) = builtin_leaf(&uri, &local) {
            return Some(leaf);
        }
        let found = self.resolve(&uri, &local, depth)?;
        leaf_of(&found)
    }

    /// Reads the leaf a declaration produces from its own `type`/`base`
    /// attribute, from a nested `xsd:restriction`, `xsd:list` or `xsd:union`.
    fn leaf(&mut self, index: usize, depth: u8) -> Option<Leaf> {
        if !self.charge(depth) {
            return None;
        }
        if depth >= MAX_TEMPLATE_DEPTH {
            self.note("depth-limit");
            return None;
        }

        if get_attr(self.dom, index, "type").is_some() {
            return self.builtin(index, "type", depth);
        }

        for child in self.dom.children(index) {
            if !is_xsd_ns(self.dom.uri(*child)) {
                continue;
            }
            let attribute = match self.dom.local(*child) {
                "restriction" => "base",
                "list" => "itemType",
                _ => continue,
            };
            if get_attr(self.dom, *child, attribute).is_some() {
                return self.builtin(*child, attribute, depth);
            }
            if self.dom.local(*child) == "union" {
                // A union names several types; the first one is the sample, because
                // nothing in the schema says which member the author intends.
                let members = get_attr(self.dom, *child, "memberTypes")?;
                let first = members.split_whitespace().next()?;
                return self.leaf_from_qname(*child, first, depth);
            }
        }

        None
    }

    /// Turns an already extracted QName into a leaf.
    fn leaf_from_qname(&mut self, child: usize, value: &str, depth: u8) -> Option<Leaf> {
        let resolved = self.dom.resolve_reference(child, value)?;
        if let Some(leaf) = builtin_leaf(&resolved.0, &resolved.1) {
            return Some(leaf);
        }
        let found = self.resolve(&resolved.0, &resolved.1, depth)?;
        leaf_of(&found)
    }

    /// Expands a reference to a globally declared element or type. A name that is
    /// already being expanded is a cycle: it is reported and printed as a comment
    /// instead of following the reference forever.
    fn resolve(&mut self, uri: &str, local: &str, depth: u8) -> Option<Definition> {
        if !self.charge(depth) {
            return None;
        }
        if depth >= MAX_TEMPLATE_DEPTH {
            self.note("depth-limit");
            return None;
        }

        let marker = key(uri, local);
        if self.active.iter().any(|known| *known == marker) {
            self.note(&format!("cycle:{local}"));
            return Some(Definition::unknown(local));
        }

        let index = self.index;
        let declaration = match index.type_definition(uri, local) {
            Some(declaration) => Some(declaration),
            None => index.element(uri, local),
        };

        let declaration = match declaration {
            Some(declaration) => declaration,
            None => {
                // Nothing in the inline schemas answers the reference. The markup
                // still shows the tag, with the type named in a comment, so the
                // user can fill it in rather than getting a failed import.
                self.note(&format!("unknown-type:{local}"));
                return Some(Definition::unknown(local));
            }
        };

        let node = declaration.node;
        let context = declaration.context.clone();

        self.active.push(marker);
        let found = self.expand(node, &context, depth);
        self.active.pop();
        found
    }

    /// Expands a declaration node: an element, a `complexType` or a `simpleType`.
    fn expand(&mut self, node: usize, context: &Context, depth: u8) -> Option<Definition> {
        match self.dom.local(node) {
            "simpleType" => {
                let leaf = self.leaf(node, depth)?;
                Some(Definition::leaf(&leaf.type_name, &leaf.sample))
            }
            "complexType" => self.complex(node, context, depth),
            "element" => self.expand_element(node, context, depth),
            _ => None,
        }
    }

    /// Expands an element declaration: the type it names, or the anonymous type
    /// written inside it. An element with neither carries nothing but text.
    fn expand_element(&mut self, node: usize, context: &Context, depth: u8) -> Option<Definition> {
        let dom = self.dom;
        let name = get_attr(dom, node, "name").unwrap_or_default();
        let found = if get_attr(dom, node, "type").is_some() {
            let (uri, local) = get_reference(dom, node, "type")?;
            match builtin_leaf(&uri, &local) {
                Some(leaf) => Definition::leaf(&leaf.type_name, &leaf.sample),
                None => self.resolve(&uri, &local, depth)?,
            }
        } else {
            let anonymous = dom.children(node).iter().copied().find(|child| {
                is_xsd_ns(dom.uri(*child))
                    && matches!(dom.local(*child), "complexType" | "simpleType")
            });
            match anonymous {
                Some(child) => self.expand(child, context, depth)?,
                None => Definition::leaf(&name, ""),
            }
        };

        Some(Definition {
            type_name: name,
            kind: found.kind,
            has_text: found.has_text,
        })
    }

    /// What a `complexType` contributes: its own attributes and children plus
    /// whatever the base of a derivation already defines.
    fn complex(&mut self, node: usize, context: &Context, depth: u8) -> Option<Definition> {
        let dom = self.dom;
        let mut fields = self.attributes_of(node, context, depth);
        let mut content = Vec::new();
        let mut base = None;

        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            match dom.local(*child) {
                "sequence" | "all" => content.extend(self.composite(*child, context, depth)),
                "choice" => content.extend(self.choice(*child, context, depth)),
                "complexContent" | "simpleContent" => {
                    base = self.derivation(*child, context, depth)
                }
                _ => {}
            }
        }

        let name = get_attr(dom, node, "name").unwrap_or_default();
        if let Some(definition) = base {
            match definition.kind {
                Kind::Composite { fields: inherited } => fields.extend(inherited),
                Kind::Choice { branches } => {
                    content.push(choice_field(branches, Occurs::default()))
                }
                Kind::Unknown => self.note(&format!("unknown-type:{}", definition.type_name)),
                Kind::Leaf { sample } => {
                    if fields.is_empty() {
                        return Some(Definition::leaf(&definition.type_name, &sample));
                    }
                    return Some(Definition {
                        type_name: definition.type_name.clone(),
                        kind: Kind::Composite { fields },
                        has_text: true,
                    });
                }
            }
        }

        fields.extend(content);
        if fields.is_empty() {
            return Some(Definition::leaf(&name, ""));
        }
        Some(Definition::composite(&name, fields))
    }

    /// A `choice` becomes a single field holding every branch, so the template can
    /// print all of them commented out and let the user keep one.
    fn choice(&mut self, node: usize, context: &Context, depth: u8) -> Vec<Field> {
        let branches = self.composite(node, context, depth);
        if branches.is_empty() {
            return Vec::new();
        }
        vec![choice_field(branches, occurs(self.dom, node))]
    }

    /// Follows the `base` of an `xsd:extension` or `xsd:restriction` written
    /// inside `complexContent`/`simpleContent`.
    fn derivation(&mut self, node: usize, _context: &Context, depth: u8) -> Option<Definition> {
        let dom = self.dom;
        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            if !matches!(dom.local(*child), "extension" | "restriction") {
                continue;
            }
            let (uri, local) = get_reference(dom, *child, "base")?;
            return self.resolve(&uri, &local, depth + 1);
        }
        None
    }

    /// Attribute declarations of a type, including those of a referenced
    /// attribute group.
    fn attributes_of(&mut self, node: usize, context: &Context, depth: u8) -> Vec<Field> {
        let dom = self.dom;
        let mut fields = Vec::new();
        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            match dom.local(*child) {
                "attribute" => fields.extend(self.attribute_field(*child, context, depth)),
                "attributeGroup" => {
                    fields.extend(self.attribute_group_ref(*child, depth).unwrap_or_default())
                }
                _ => {}
            }
        }
        fields
    }

    /// One attribute declaration. Its value is printed inside the tag of the
    /// element that carries it, so only a leaf value is useful here.
    fn attribute_field(&mut self, node: usize, context: &Context, depth: u8) -> Option<Field> {
        let dom = self.dom;
        let name = get_attr(dom, node, "name")?;
        let leaf = if get_attr(dom, node, "type").is_some() {
            self.builtin(node, "type", depth)
        } else {
            self.leaf(node, depth + 1)
        };

        let (type_name, kind) = match leaf {
            Some(leaf) => (
                leaf.type_name,
                Kind::Leaf {
                    sample: leaf.sample,
                },
            ),
            None => (String::new(), Kind::Unknown),
        };

        Some(Field {
            uri: context.namespace_for(&context.attribute_form, depth),
            local: name,
            type_name,
            kind,
            // An attribute is optional unless the schema says otherwise.
            occurs: Occurs {
                min: u8::from(get_attr(dom, node, "use").as_deref() == Some("required")),
                max: 1,
            },
            attribute: true,
            nillable: false,
        })
    }

    fn attribute_group_ref(&mut self, node: usize, depth: u8) -> Option<Vec<Field>> {
        let (uri, local) = get_reference(self.dom, node, "ref")?;
        let declaration = self.index.attribute_group(&uri, &local)?;
        let group = declaration.node;
        let context = declaration.context.clone();
        Some(self.attributes_of(group, &context, depth))
    }

    /// Children of a `sequence` or `all`, flattened into fields in the order the
    /// schema declares them.
    fn composite(&mut self, node: usize, context: &Context, depth: u8) -> Vec<Field> {
        let dom = self.dom;
        let mut fields = Vec::new();
        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            match dom.local(*child) {
                "element" => fields.extend(self.element_field(*child, context, depth)),
                "sequence" | "all" => fields.extend(self.composite(*child, context, depth + 1)),
                "choice" => fields.extend(self.choice(*child, context, depth + 1)),
                "group" => fields.extend(self.group_ref(*child, depth).unwrap_or_default()),
                "any" => {
                    // `xsd:any` says "some element from somewhere": there is nothing
                    // to guess, so the template comments it and the user fills it in.
                    fields.push(Field {
                        uri: String::new(),
                        local: "any".to_string(),
                        type_name: "xsd:any".to_string(),
                        kind: Kind::Unknown,
                        occurs: occurs(dom, *child),
                        attribute: false,
                        nillable: false,
                    });
                }
                _ => {}
            }
        }
        fields
    }

    /// One child element of a composite type. A local element takes its namespace
    /// from the schema's `elementFormDefault`; a `ref` takes it from the global
    /// declaration it points at, which is always in that schema's namespace.
    fn element_field(&mut self, node: usize, context: &Context, depth: u8) -> Option<Field> {
        let dom = self.dom;

        if let Some((uri, local)) = get_reference(dom, node, "ref") {
            let declaration = self.index.element(&uri, &local)?;
            let namespace = declaration.context.target_ns.clone();
            let found = self.resolve(&uri, &local, depth + 1)?;
            return Some(Field {
                uri: namespace,
                local,
                type_name: found.type_name,
                kind: found.kind,
                occurs: occurs(dom, node),
                attribute: false,
                nillable: get_attr(dom, node, "nillable").as_deref() == Some("true"),
            });
        }

        let name = get_attr(dom, node, "name")?;
        let found = if get_attr(dom, node, "type").is_some() {
            let (uri, local) = get_reference(dom, node, "type")?;
            match builtin_leaf(&uri, &local) {
                Some(leaf) => Definition::leaf(&leaf.type_name, &leaf.sample),
                None => self.resolve(&uri, &local, depth + 1)?,
            }
        } else if let Some(anonymous) = dom.children(node).iter().copied().find(|child| {
            is_xsd_ns(dom.uri(*child)) && matches!(dom.local(*child), "complexType" | "simpleType")
        }) {
            self.expand(anonymous, context, depth + 1)?
        } else {
            Definition::leaf(&name, "")
        };

        Some(Field {
            uri: context.namespace_for(&context.element_form, depth),
            local: name,
            type_name: found.type_name,
            kind: found.kind,
            occurs: occurs(dom, node),
            attribute: false,
            nillable: get_attr(dom, node, "nillable").as_deref() == Some("true"),
        })
    }

    /// A `ref` to a globally declared group contributes the markup the group
    /// defines, in the namespace of the schema that declares it.
    fn group_ref(&mut self, node: usize, depth: u8) -> Option<Vec<Field>> {
        let (uri, local) = get_reference(self.dom, node, "ref")?;
        let declaration = self.index.group(&uri, &local)?;
        let group = declaration.node;
        let context = declaration.context.clone();
        Some(self.group_body(group, &context, depth))
    }

    fn group_body(&mut self, node: usize, context: &Context, depth: u8) -> Vec<Field> {
        let dom = self.dom;
        let mut fields = Vec::new();
        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            match dom.local(*child) {
                "sequence" | "all" => fields.extend(self.composite(*child, context, depth)),
                "choice" => fields.extend(self.choice(*child, context, depth)),
                _ => {}
            }
        }
        fields
    }
}

/// A field whose markup is a set of alternatives rather than one element.
fn choice_field(branches: Vec<Field>, occurs: Occurs) -> Field {
    Field {
        uri: String::new(),
        local: String::new(),
        type_name: String::new(),
        kind: Kind::Choice { branches },
        occurs,
        attribute: false,
        nillable: false,
    }
}

/// Turns a resolved definition into a leaf value. A definition that has children
/// is not a leaf, and `Unknown` keeps its type name as the comment text.
fn leaf_of(found: &Definition) -> Option<Leaf> {
    match &found.kind {
        Kind::Leaf { sample } => Some(Leaf {
            type_name: found.type_name.clone(),
            sample: sample.clone(),
        }),
        Kind::Unknown => Some(Leaf {
            type_name: found.type_name.clone(),
            sample: String::new(),
        }),
        _ => None,
    }
}

/// One globally declared element or type, remembered as an arena index plus the
/// namespace rules of the schema that declares it.
struct Declaration {
    node: usize,
    context: Context,
}

/// The declarations of every inline schema in the document. Lookup is keyed by
/// namespace URI and local name, because that is what a resolved QName gives —
/// the prefix a document uses carries no meaning.
pub struct XsdIndex {
    /// Global element declarations, keyed by their schema's target namespace.
    elements: HashMap<String, Declaration>,
    /// Global `complexType` and `simpleType` declarations.
    types: HashMap<String, Declaration>,
    /// Global `group` and `attributeGroup` declarations, which a `ref` inside a
    /// sequence or an attribute list points at.
    groups: HashMap<String, Declaration>,
    attribute_groups: HashMap<String, Declaration>,
    /// Local names that exist in some target namespace, for references written
    /// without a prefix inside a document that declares one.
    by_local: HashMap<String, Vec<(String, usize)>>,
    /// Target namespaces of the inline schemas, in document order.
    target_namespaces: Vec<String>,
    schemas: Vec<usize>,
}

impl XsdIndex {
    /// Collects the declarations of every `xsd:schema` in the document. Imported
    /// schemas are not reachable: a document that imports one reports a warning
    /// and prints whatever it can from the inline part.
    pub fn build(dom: &XmlDom) -> XsdIndex {
        let mut index = XsdIndex {
            elements: HashMap::new(),
            types: HashMap::new(),
            groups: HashMap::new(),
            attribute_groups: HashMap::new(),
            by_local: HashMap::new(),
            target_namespaces: Vec::new(),
            schemas: Vec::new(),
        };

        for schema in dom.find(XSD_NAMESPACE, "schema") {
            index.schemas.push(schema);
            let context = Context::of(dom, schema);
            if !context.target_ns.is_empty()
                && !index
                    .target_namespaces
                    .iter()
                    .any(|known| known == &context.target_ns)
            {
                index.target_namespaces.push(context.target_ns.clone());
            }
            for child in dom.children(schema) {
                if !is_xsd_ns(dom.uri(*child)) {
                    continue;
                }
                let name = match get_attr(dom, *child, "name") {
                    Some(name) => name,
                    None => continue,
                };
                let target = match dom.local(*child) {
                    "element" => &mut index.elements,
                    "complexType" | "simpleType" => &mut index.types,
                    "group" => &mut index.groups,
                    "attributeGroup" => &mut index.attribute_groups,
                    _ => continue,
                };
                target.insert(
                    key(&context.target_ns, &name),
                    Declaration {
                        node: *child,
                        context: context.clone(),
                    },
                );
                index
                    .by_local
                    .entry(name)
                    .or_default()
                    .push((context.target_ns.clone(), *child));
            }
        }

        index
    }

    /// A global element by namespace and name.
    fn element(&self, uri: &str, local: &str) -> Option<&Declaration> {
        lookup(&self.elements, &self.by_local, uri, local)
    }

    fn type_definition(&self, uri: &str, local: &str) -> Option<&Declaration> {
        lookup(&self.types, &self.by_local, uri, local)
    }

    fn group(&self, uri: &str, local: &str) -> Option<&Declaration> {
        lookup(&self.groups, &self.by_local, uri, local)
    }

    fn attribute_group(&self, uri: &str, local: &str) -> Option<&Declaration> {
        lookup(&self.attribute_groups, &self.by_local, uri, local)
    }
}

/// The body fragment of a request plus the warnings found while resolving it.
pub struct TemplateFragment {
    pub xml: String,
    pub warnings: Vec<String>,
}

/// Records a namespace once, keeping the order in which the template uses them.
fn add_uri(uri: &str, out: &mut Vec<String>) {
    if !uri.is_empty() && !out.iter().any(|known| known == uri) {
        out.push(uri.to_string());
    }
}

/// Namespaces the template needs, in the order they are first used.
fn collect_uris(kind: &Kind, uri: &str, out: &mut Vec<String>) {
    add_uri(uri, out);
    match kind {
        Kind::Leaf { .. } | Kind::Unknown => {}
        Kind::Composite { fields } => {
            for field in fields {
                add_uri(&field.uri, out);
                collect_uris(&field.kind, &field.uri, out);
            }
        }
        Kind::Choice { branches } => {
            for branch in branches {
                add_uri(&branch.uri, out);
                collect_uris(&branch.kind, &branch.uri, out);
            }
        }
    }
}

/// Prefixes for every namespace the template uses. A prefix the WSDL document
/// already declares is reused, so the fragment reads like the document it came
/// from; an undeclared namespace gets a generated prefix and its own declaration.
fn assign_prefixes(dom: &XmlDom, scope: usize, uris: &[String]) -> Vec<(String, String)> {
    let mut bound: Vec<(String, String)> = Vec::new();
    let mut generated = 0usize;

    for uri in uris {
        let prefix = match dom
            .binding(scope, uri)
            .or_else(|| dom.binding(dom.root(), uri))
        {
            Some(prefix) if !prefix.is_empty() => prefix,
            _ => {
                generated += 1;
                format!("{GENERATED_PREFIX}{generated}")
            }
        };
        if !bound.iter().any(|(known, _)| known == &prefix) {
            bound.push((prefix, uri.clone()));
        }
    }

    bound
}

/// Makes a line safe to put inside a comment: XML comments cannot contain `--`.
fn soften(text: &str) -> String {
    text.replace("--", "- -")
}

/// One comment line of a template. Every note goes through here so a name or a
/// built-in sample that holds a double hyphen (`--01` of `xsd:gMonth`) can never
/// break the comment it is printed in.
fn note(pad: &str, text: &str) -> String {
    format!("{pad}<!-- {} -->", soften(text))
}

/// Comment body of one generated line. A note that is already a comment — the
/// nillable and cardinality notes of a child reached through an optional parent —
/// contributes only its text: a comment cannot hold another one, and wrapping it
/// twice is what produced the `<!-- <!-- x --> -->` and the softened
/// `<!- - … - ->` garbage.
fn comment_body(line: &str) -> String {
    let mut text = line.trim();
    // Peel whole-line comment wrappers as many times as they are nested.
    while text.len() > "<!---->".len() && text.starts_with("<!--") && text.ends_with("-->") {
        text = text["<!--".len()..text.len() - "-->".len()].trim();
    }
    soften(text.replace("<!--", "").replace("-->", "").trim())
}

/// Wraps markup so the user sees it as a block they can uncomment. Each line
/// keeps its own indentation, so an uncommented block nests the way the schema
/// does. A line with nothing to say produces no comment: an empty comment is
/// noise, and `<!---->` is not markup.
fn comment_block(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let body = comment_body(line);
            let pad = line.len() - line.trim_start().len();
            (!body.is_empty()).then(|| format!("{}<!-- {body} -->", &line[..pad]))
        })
        .collect()
}

/// Prints a field tree with the prefixes assigned to its namespaces.
struct Printer {
    bound: Vec<(String, String)>,
    /// `xmlns` declarations of the root element, so the fragment carries every
    /// namespace it uses rather than relying on the WSDL document.
    declarations: String,
}

impl Printer {
    fn qualified(&self, uri: &str, local: &str) -> String {
        match self.bound.iter().find(|(_, bound)| bound == uri) {
            Some((prefix, _)) if !prefix.is_empty() => format!("{prefix}:{local}"),
            _ => local.to_string(),
        }
    }

    /// Attributes printed inside the tag: only the required ones, because an
    /// optional attribute would make the element invalid when left unfilled.
    fn inline_attributes(&self, field: &Field) -> String {
        let mut text = String::new();
        for child in fields_of(&field.kind) {
            if !child.attribute || child.occurs.min == 0 {
                continue;
            }
            let value = match &child.kind {
                Kind::Leaf { sample } => sample.clone(),
                _ => String::new(),
            };
            text.push_str(&format!(
                " {}=\"{}\"",
                self.qualified(&child.uri, &child.local),
                value
            ));
        }
        text
    }

    /// Comments naming the attributes the user may add, printed above the element.
    fn attribute_notes(&self, field: &Field, indent: usize) -> Vec<String> {
        let pad = "  ".repeat(indent);
        fields_of(&field.kind)
            .iter()
            .filter(|child| child.attribute && child.occurs.min == 0)
            .map(|child| {
                let value = match &child.kind {
                    Kind::Leaf { sample } => format!("=\"{sample}\""),
                    _ => String::new(),
                };
                note(
                    &pad,
                    &format!(
                        "optional attribute {}{}",
                        self.qualified(&child.uri, &child.local),
                        value
                    ),
                )
            })
            .collect()
    }
}
/// Element children a kind carries. Attributes are not returned here: they print
/// inside the tag of their parent rather than as markup of their own.
fn fields_of(kind: &Kind) -> &[Field] {
    match kind {
        Kind::Composite { fields } => &fields,
        _ => &[],
    }
}

impl Printer {
    /// Markup of one field, with nothing commented yet.
    fn markup(&self, field: &Field, indent: usize) -> Vec<String> {
        let pad = "  ".repeat(indent);
        let tag = self.qualified(&field.uri, &field.local);
        let mut lines = self.attribute_notes(field, indent);
        if field.nillable {
            lines.push(note(
                &pad,
                &format!("{tag} is nillable: write xsi:nil=\"true\" to send no value"),
            ));
        }

        let attributes = if indent == 0 && !field.attribute {
            format!("{}{}", self.inline_attributes(field), self.declarations)
        } else {
            self.inline_attributes(field)
        };

        match &field.kind {
            Kind::Unknown => {
                let label = match (field.local.as_str(), field.type_name.as_str()) {
                    ("", "") => "element".to_string(),
                    ("", name) => name.to_string(),
                    (name, "") => name.to_string(),
                    (name, type_name) => format!("{name} ({type_name})"),
                };
                lines.push(note(
                    &pad,
                    &format!("{label}: content could not be generated"),
                ));
            }
            Kind::Leaf { sample } => {
                lines.push(format!("{pad}<{tag}{attributes}>{sample}</{tag}>"));
            }
            Kind::Composite { fields } => {
                let children: Vec<String> = fields
                    .iter()
                    .filter(|child| !child.attribute)
                    .flat_map(|child| self.entry(child, indent + 1))
                    .collect();
                if children.is_empty() {
                    lines.push(format!("{pad}<{tag}{attributes}></{tag}>"));
                } else {
                    lines.push(format!("{pad}<{tag}{attributes}>"));
                    lines.extend(children);
                    lines.push(format!("{pad}</{tag}>"));
                }
            }
            Kind::Choice { branches } => {
                lines.push(note(
                    &pad,
                    &format!("choose one of {} alternatives", branches.len()),
                ));
                for branch in branches {
                    lines.extend(comment_block(&self.entry(branch, indent)));
                }
            }
        }

        lines
    }

    /// Markup of one field with its cardinality applied: an optional block comes
    /// commented out, a repeating one appears twice with a note to copy it.
    fn entry(&self, field: &Field, indent: usize) -> Vec<String> {
        let pad = "  ".repeat(indent);

        if field.occurs.min == 0 {
            let mut lines = vec![note(
                &pad,
                &format!("optional: {}", self.qualified(&field.uri, &field.local)),
            )];
            lines.extend(comment_block(&self.markup(field, indent)));
            return lines;
        }

        if field.occurs.max == UNBOUNDED || field.occurs.max > 1 {
            let mut lines = Vec::new();
            for _ in 0..REPETITIONS {
                lines.extend(self.markup(field, indent));
            }
            lines.push(note(&pad, "repeat as needed"));
            return lines;
        }

        self.markup(field, indent)
    }
}

/// The body fragment of one WSDL part element. The head element carries the
/// `xmlns` declarations of every namespace the fragment uses, which is what makes
/// it paste-ready into a SOAP envelope without the WSDL document beside it.
///
/// A type that cannot be resolved produces a comment and a warning instead of a
/// failed import, so the user still gets an editable skeleton.
pub fn body_template(
    dom: &XmlDom,
    index: &XsdIndex,
    scope: usize,
    uri: &str,
    local: &str,
) -> TemplateFragment {
    let mut walk = Walk::new(dom, index);

    let declaration = match index.element(uri, local) {
        Some(declaration) => declaration,
        None => {
            walk.note(&format!("unknown-type:{local}"));
            return TemplateFragment {
                xml: String::new(),
                warnings: walk.warnings,
            };
        }
    };

    let node = declaration.node;
    let context = declaration.context.clone();
    let found = match walk.expand_element(node, &context, 1) {
        Some(found) => found,
        None => {
            walk.note(&format!("unknown-type:{local}"));
            return TemplateFragment {
                xml: String::new(),
                warnings: walk.warnings,
            };
        }
    };

    // A global element is always in its schema's namespace, whatever
    // `elementFormDefault` says about the elements it contains.
    let head = Field {
        uri: context.target_ns.clone(),
        local: local.to_string(),
        type_name: found.type_name,
        kind: found.kind,
        occurs: Occurs::default(),
        attribute: false,
        nillable: get_attr(dom, node, "nillable").as_deref() == Some("true"),
    };

    let mut uris = Vec::new();
    collect_uris(&head.kind, &head.uri, &mut uris);
    let bound = assign_prefixes(dom, scope, &uris);

    let mut declarations = String::new();
    for (prefix, namespace) in &bound {
        if prefix.is_empty() {
            declarations.push_str(&format!(" xmlns=\"{namespace}\""));
        } else {
            declarations.push_str(&format!(" xmlns:{prefix}=\"{namespace}\""));
        }
    }

    let printer = Printer {
        bound,
        declarations,
    };
    let lines = printer.entry(&head, 0);

    TemplateFragment {
        xml: lines.join("\n"),
        warnings: walk.warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::parser::{build_dom, parse_wsdl};

    /// A chain of optional, nillable elements across two schemas — the shape that
    /// makes a note line of a child reach a comment of its parent.
    const SAMPLE: &str = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
      xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
      xmlns:xsd="http://www.w3.org/2001/XMLSchema"
      xmlns:tns="http://nemo-ibe.com/Avia"
      xmlns:ns1="http://nemo-ibe.com/STL"
      targetNamespace="http://nemo-ibe.com/Avia">
  <wsdl:types>
    <xsd:schema targetNamespace="http://nemo-ibe.com/Avia" elementFormDefault="qualified">
      <xsd:element name="CloseSession" type="tns:CloseSessionRequest"/>
      <xsd:complexType name="CloseSessionRequest">
        <xsd:sequence>
          <xsd:element name="Request" type="ns1:Request" minOccurs="0" nillable="true"/>
        </xsd:sequence>
      </xsd:complexType>
    </xsd:schema>
    <xsd:schema targetNamespace="http://nemo-ibe.com/STL" elementFormDefault="qualified">
      <xsd:complexType name="Request">
        <xsd:sequence>
          <xsd:element name="Requisites" type="ns1:Requisites" minOccurs="0" nillable="true"/>
          <xsd:choice>
            <xsd:element name="TicketRef" type="xsd:string" minOccurs="0" nillable="true"/>
            <xsd:element name="OrderRef" type="xsd:string" minOccurs="0"/>
          </xsd:choice>
          <xsd:element name="RequestType" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="Requisites">
        <xsd:sequence>
          <xsd:element name="Login" type="ns1:Login" minOccurs="0" nillable="true"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="Login">
        <xsd:sequence>
          <xsd:element name="Password" type="xsd:string" minOccurs="0" nillable="true"/>
          <xsd:element name="SessionId" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="CloseSessionInput">
    <wsdl:part name="body" element="tns:CloseSession"/>
  </wsdl:message>
  <wsdl:portType name="AviaPortType">
    <wsdl:operation name="CloseSession">
      <wsdl:input message="tns:CloseSessionInput"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="AviaBinding" type="tns:AviaPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="CloseSession">
      <soap:operation soapAction="urn:CloseSession" style="document"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="AviaService">
    <wsdl:port name="AviaPort" binding="tns:AviaBinding">
      <soap:address location="http://127.0.0.1:9000/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    fn template_of(source: &str) -> String {
        let parsed = parse_wsdl(source).expect("wsdl");
        let index = XsdIndex::build(&parsed.dom);
        body_template(
            &parsed.dom,
            &index,
            parsed.dom.root(),
            "http://nemo-ibe.com/Avia",
            "CloseSession",
        )
        .xml
    }

    #[test]
    fn comment_body_drops_the_markers_of_an_already_commented_line() {
        assert_eq!(
            comment_body("<!-- optional: ns1:Login -->"),
            "optional: ns1:Login"
        );
        assert_eq!(
            comment_body("<!-- <!-- ns1:Login is nillable --> -->"),
            "ns1:Login is nillable"
        );
        assert_eq!(
            comment_body("  <ns1>Login>text</ns1>Login>  "),
            "<ns1>Login>text</ns1>Login>"
        );
        // An unclosed opener is text rather than a comment; the markers still go.
        assert_eq!(comment_body("<!-- a -- b"), "a - - b");
    }

    #[test]
    fn comment_block_never_emits_an_empty_or_nested_comment() {
        let lines = vec![
            "   ".to_string(),
            "<!-- <!-- optional: ns1:Login --> -->".to_string(),
            "<a>x</a>".to_string(),
        ];
        assert_eq!(
            comment_block(&lines),
            vec![
                "<!-- optional: ns1:Login -->".to_string(),
                "<!-- <a>x</a> -->".to_string()
            ]
        );
        assert_eq!(
            comment_block(&["<a>--</a>".to_string()]),
            vec!["<!-- <a>- -</a> -->".to_string()]
        );
        assert_eq!(
            comment_block(&["    <a>x</a>".to_string()]),
            vec!["    <!-- <a>x</a> -->".to_string()]
        );
        assert!(comment_block(&["<!---->".to_string()]).is_empty());
    }

    #[test]
    fn a_note_survives_a_name_or_sample_with_double_hyphens() {
        assert_eq!(note("  ", "gMonth is --01"), "  <!-- gMonth is - -01 -->");
        assert_eq!(note("", "optional: ns:a--b"), "<!-- optional: ns:a- -b -->");
    }

    #[test]
    fn prints_one_comment_per_generated_line() {
        let xml = template_of(SAMPLE);
        for line in xml.lines() {
            assert!(
                line.matches("<!--").count() <= 1,
                "nested comment markers: {line}"
            );
            assert!(
                line.matches("-->").count() <= 1,
                "nested comment markers: {line}"
            );
            assert!(!line.contains("<!- -"), "softened opener: {line}");
            assert!(!line.contains("- ->"), "softened closer: {line}");
            assert!(!line.contains("<!---->"), "empty comment: {line}");
        }
    }

    #[test]
    fn prints_markup_that_reads_as_xml() {
        build_dom(&template_of(SAMPLE)).expect("template is well-formed markup");
    }

    #[test]
    fn keeps_the_indentation_of_a_commented_block() {
        let xml = template_of(SAMPLE);
        assert!(xml.contains("  <!-- <tns:Request> -->"), "{xml}");
        assert!(xml.contains("      <!-- <ns1:Login> -->"), "{xml}");
        assert!(xml.contains("      <!-- </ns1:Login> -->"), "{xml}");
        assert!(xml.contains("  <!-- </tns:Request> -->"), "{xml}");
    }

    #[test]
    fn keeps_the_notes_of_a_nested_optional_element_readable() {
        let xml = template_of(SAMPLE);
        assert!(xml.contains("<tns:CloseSession xmlns:tns="), "{xml}");
        assert!(xml.contains("<!-- optional: tns:Request -->"), "{xml}");
        assert!(
            xml.contains(
                "<!-- tns:Request is nillable: write xsi:nil=\"true\" to send no value -->"
            ),
            "{xml}"
        );
        assert!(xml.contains("<!-- <ns1:Login> -->"), "{xml}");
        assert!(xml.contains("<!-- </ns1:Login> -->"), "{xml}");
        assert!(
            xml.contains(
                "<!-- ns1:Password is nillable: write xsi:nil=\"true\" to send no value -->"
            ),
            "{xml}"
        );
        assert!(
            xml.contains("<!-- <ns1:SessionId>text</ns1:SessionId> -->"),
            "{xml}"
        );
        // Every commented block says each thing once.
        assert_eq!(
            xml.matches("<!-- optional: ns1:Login -->").count(),
            1,
            "{xml}"
        );
    }

    #[test]
    fn flattens_the_notes_of_a_choice_branch_inside_an_optional_element() {
        let xml = template_of(SAMPLE);
        assert!(
            xml.contains("<!-- choose one of 2 alternatives -->"),
            "{xml}"
        );
        assert_eq!(
            xml.matches("<!-- optional: ns1:TicketRef -->").count(),
            1,
            "{xml}"
        );
        assert_eq!(
            xml.matches(
                "<!-- ns1:TicketRef is nillable: write xsi:nil=\"true\" to send no value -->"
            )
            .count(),
            1,
            "{xml}"
        );
        assert_eq!(
            xml.matches("<!-- <ns1:OrderRef>text</ns1:OrderRef> -->")
                .count(),
            1,
            "{xml}"
        );
    }
}
