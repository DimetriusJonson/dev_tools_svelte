use std::collections::HashMap;

use crate::wsdl::parser::{get_attr, get_reference, is_xsd_ns, XmlDom};
use crate::wsdl::{MAX_TEMPLATE_DEPTH, MAX_TEMPLATE_ELEMENTS, XSD_NAMESPACE};

/// Separator inside definition keys. It cannot appear in a namespace URI or in an
/// XSD name, so a key splits back into its two parts without ambiguity.
const KEY_SEPARATOR: char = '\u{0001}';

/// `maxOccurs="unbounded"` is kept as zero, which means "no cap is implied".
const UNBOUNDED: u8 = 0;

/// A finite `maxOccurs` prints at most this many copies of the element: the user
/// sees that the block is a list and copies it for the rest.
const REPETITIONS: usize = 2;

/// Prefix used for a namespace the schema never declared. Printing the URI as a
/// prefix would be unreadable, and printing nothing silently changes meaning.
const GENERATED_PREFIX: &str = "ns";

/// Value of every leaf, attribute and text: SoapUI writes `?` and the user
/// replaces it with test data, so no plausible-looking sample is invented.
const PLACEHOLDER: &str = "?";

/// SoapUI's own markers above a block, spelled exactly as SoapUI spells them.
/// An optional element is *not* commented out — the marker says it may go.
const OPTIONAL_NOTE: &str = "Optional:";
const ZERO_OR_MORE: &str = "Zero or more repetitions:";
const ONE_OR_MORE: &str = "One or more repetitions:";
const CHOICE_NOTE: &str = "Choice:";

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
    /// Text the element carries beside its attributes: a `simpleContent` extension
    /// of a simple type has attributes *and* a value. `false` for markup that has
    /// no text of its own.
    text: bool,
}

/// What a resolved declaration contributes to the template.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    /// Text content, printed as the placeholder.
    Leaf,
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
    /// Text a type carries alongside attributes, taken from the base of a
    /// `simpleContent` extension. Markup without a value of its own is `false`.
    text: bool,
}

impl Definition {
    fn unknown(type_name: &str) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Unknown,
            text: false,
        }
    }

    fn leaf(type_name: &str) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Leaf,
            // A leaf prints the placeholder; `text` is only the value a
            // composite carries beside its attributes.
            text: false,
        }
    }

    fn composite(type_name: &str, fields: Vec<Field>, text: bool) -> Definition {
        Definition {
            type_name: type_name.to_string(),
            kind: Kind::Composite { fields },
            text,
        }
    }
}

/// What a `complexContent`/`simpleContent` block contributes: the type named on
/// `base` plus the attributes and the content model the derivation itself adds.
struct Derivation {
    base: Option<Definition>,
    attributes: Vec<Field>,
    content: Vec<Field>,
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
    if !is_xsd_ns(uri) || !builtin_type(local) {
        return None;
    }
    Some(Leaf {
        type_name: local.to_string(),
    })
}

/// A leaf value: the element's own type, or the item type of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Leaf {
    /// `xsd:string` and friends, or the name of a user type when nothing builtin
    /// could be reached — that name becomes the comment text.
    type_name: String,
}

/// The built-in types a template can print. Only whether the name is known
/// matters: the value itself is always the placeholder.
fn builtin_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "string"
            | "ID"
            | "IDREF"
            | "IDREFS"
            | "NCName"
            | "NMTOKEN"
            | "NMTOKENS"
            | "QName"
            | "normalizedString"
            | "token"
            | "base64Binary"
            | "hexBinary"
            | "boolean"
            | "integer"
            | "int"
            | "long"
            | "short"
            | "byte"
            | "nonNegativeInteger"
            | "positiveInteger"
            | "nonPositiveInteger"
            | "negativeInteger"
            | "unsignedInt"
            | "unsignedLong"
            | "unsignedShort"
            | "unsignedByte"
            | "decimal"
            | "float"
            | "double"
            | "date"
            | "time"
            | "dateTime"
            | "duration"
            | "gYear"
            | "gMonth"
            | "gDay"
            | "language"
    )
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
                // A union names several types; the first one is printed, because
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
                Some(Definition::leaf(&leaf.type_name))
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
                Some(leaf) => Definition::leaf(&leaf.type_name),
                None => self.resolve(&uri, &local, depth)?,
            }
        } else {
            let anonymous = dom.children(node).iter().copied().find(|child| {
                is_xsd_ns(dom.uri(*child))
                    && matches!(dom.local(*child), "complexType" | "simpleType")
            });
            match anonymous {
                Some(child) => self.expand(child, context, depth)?,
                None => Definition::leaf(&name),
            }
        };

        Some(Definition {
            type_name: name,
            kind: found.kind,
            text: found.text,
        })
    }

    /// What a `complexType` contributes: its own attributes and children plus
    /// whatever the base of a derivation already defines.
    fn complex(&mut self, node: usize, context: &Context, depth: u8) -> Option<Definition> {
        let dom = self.dom;
        let mut fields = self.attributes_of(node, context, depth);
        let mut content = Vec::new();
        let mut derivation: Option<Derivation> = None;

        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            match dom.local(*child) {
                "sequence" | "all" => content.extend(self.composite(*child, context, depth)),
                "choice" => content.extend(self.choice(*child, context, depth)),
                "complexContent" | "simpleContent" => {
                    derivation = self.derivation(*child, context, depth)
                }
                _ => {}
            }
        }

        let name = get_attr(dom, node, "name").unwrap_or_default();
        let mut text = false;

        if let Some(Derivation {
            base,
            attributes,
            content: added,
        }) = derivation
        {
            match base {
                Some(definition) => match definition.kind {
                    // The base's markup comes first: an extension appends what it
                    // declares after it. A restriction that cuts the base is only
                    // over-generated here, which is what a starting point means —
                    // the user edits the block out rather than losing the base.
                    Kind::Composite { fields: inherited } => {
                        fields.extend(inherited);
                        fields.extend(added);
                        text = definition.text;
                    }
                    Kind::Choice { branches } => {
                        content.push(choice_field(branches, Occurs::default()));
                        content.extend(added);
                    }
                    Kind::Unknown => self.note(&format!("unknown-type:{}", definition.type_name)),
                    Kind::Leaf => {
                        if fields.is_empty()
                            && attributes.is_empty()
                            && content.is_empty()
                            && added.is_empty()
                        {
                            return Some(Definition::leaf(&definition.type_name));
                        }
                        // The value the base's simple type carries stays printable
                        // beside the attributes the derivation adds.
                        text = true;
                        content.extend(added);
                    }
                },
                None => content.extend(added),
            }
            fields.extend(attributes);
        }

        fields.extend(content);
        if fields.is_empty() {
            return Some(Definition::leaf(&name));
        }
        Some(Definition::composite(&name, fields, text))
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
    /// inside `complexContent`/`simpleContent`, and collects what the derivation
    /// itself adds: attributes and a content model of its own. Reading only the
    /// `base` would drop the added markup, which is where a derived request type
    /// loses the body element it declares over its base.
    fn derivation(&mut self, node: usize, context: &Context, depth: u8) -> Option<Derivation> {
        let dom = self.dom;
        for child in dom.children(node) {
            if !is_xsd_ns(dom.uri(*child)) {
                continue;
            }
            if !matches!(dom.local(*child), "extension" | "restriction") {
                continue;
            }
            let base = get_reference(dom, *child, "base").and_then(|(uri, local)| {
                match builtin_leaf(&uri, &local) {
                    // A derivation of a built-in type is a printable leaf, so
                    // resolving it as a global declaration would report a type the
                    // XSD namespace defines as unknown.
                    Some(leaf) => Some(Definition::leaf(&leaf.type_name)),
                    None => self.resolve(&uri, &local, depth + 1),
                }
            });
            // The base first: it is the shorter half of a derivation, so spending
            // the element budget on a deep body before reaching it would leave the
            // requisites of the base type out of an otherwise long template.
            let attributes = self.attributes_of(*child, context, depth);
            // The derivation's own group children, in the namespace of the schema
            // that writes them rather than that of the base.
            let content = self.composite(*child, context, depth + 1);
            return Some(Derivation {
                base,
                attributes,
                content,
            });
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
            Some(leaf) => (leaf.type_name, Kind::Leaf),
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
            text: false,
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
                        text: false,
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
                text: found.text,
            });
        }

        let name = get_attr(dom, node, "name")?;
        let found = if get_attr(dom, node, "type").is_some() {
            let (uri, local) = get_reference(dom, node, "type")?;
            match builtin_leaf(&uri, &local) {
                Some(leaf) => Definition::leaf(&leaf.type_name),
                None => self.resolve(&uri, &local, depth + 1)?,
            }
        } else if let Some(anonymous) = dom.children(node).iter().copied().find(|child| {
            is_xsd_ns(dom.uri(*child)) && matches!(dom.local(*child), "complexType" | "simpleType")
        }) {
            self.expand(anonymous, context, depth + 1)?
        } else {
            Definition::leaf(&name)
        };

        Some(Field {
            uri: context.namespace_for(&context.element_form, depth),
            local: name,
            type_name: found.type_name,
            kind: found.kind,
            occurs: occurs(dom, node),
            attribute: false,
            text: found.text,
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
        text: false,
    }
}

/// Turns a resolved definition into a leaf. A definition that has children
/// is not a leaf, and `Unknown` keeps its type name as the comment text.
fn leaf_of(found: &Definition) -> Option<Leaf> {
    match &found.kind {
        Kind::Leaf => Some(Leaf {
            type_name: found.type_name.clone(),
        }),
        Kind::Unknown => Some(Leaf {
            type_name: found.type_name.clone(),
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

/// One explanatory comment line of a template. Every note goes through here so a
/// type or element name that holds a double hyphen can never break the comment it
/// is printed in.
fn note(pad: &str, text: &str) -> String {
    format!("{pad}<!-- {} -->", soften(text))
}

/// SoapUI's marker above a block, printed without the spaces of [`note`] so the
/// template reads like the one SoapUI writes (`<!--Optional:-->`).
fn marker(pad: &str, text: &str) -> String {
    format!("{pad}<!--{}-->", soften(text))
}

/// A whole-line marker comment (`<!--Optional:-->`). It is a comment already, so
/// wrapping it again would nest markers for no reason. `<!---->` is an empty
/// comment rather than a marker, and a note written with spaces (`<!-- x -->`)
/// is not one either: only SoapUI's tight spelling is. Returns the text inside.
fn is_marker(line: &str) -> Option<&str> {
    let text = line.trim();
    if text.len() <= "<!---->".len()
        || !text.starts_with("<!--")
        || !text.ends_with("-->")
        || text.starts_with("<!-- ")
    {
        return None;
    }
    Some(text["<!--".len()..text.len() - "-->".len()].trim())
}

/// The text of one generated line with its comment wrappers peeled off, and
/// whether the innermost line was written in SoapUI's tight marker spelling. A
/// note that is already a comment — the marker of a child reached through a
/// commented `choice` branch — contributes only its text: a comment cannot hold
/// another one, and wrapping it twice is what produced the `<!-- <!-- x --> -->`
/// and the softened `<!- - … - ->` garbage.
fn comment_text(line: &str) -> (String, bool) {
    let mut text = line.trim();
    let mut tight = false;
    loop {
        if let Some(body) = is_marker(text) {
            tight = true;
            text = body;
        } else if text.len() > "<!---->".len() && text.starts_with("<!--") && text.ends_with("-->")
        {
            text = text["<!--".len()..text.len() - "-->".len()].trim();
        } else {
            break;
        }
    }
    (
        soften(text.replace("<!--", "").replace("-->", "").trim()),
        tight,
    )
}

/// Comment body of one generated line, markers stripped and double hyphens made
/// safe. Kept separate from [`comment_text`] so the flattening stays testable.
fn comment_body(line: &str) -> String {
    comment_text(line).0
}

/// Wraps markup so the user sees it as a block they can uncomment. Each line
/// keeps its own indentation, so an uncommented block nests the way the schema
/// does. A marker line is a comment of its own: it stays tight and uncommented
/// rather than being wrapped again. A line with nothing to say produces no
/// comment: an empty comment is noise, and `<!---->` is not markup.
fn comment_block(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let pad = &line[..line.len() - line.trim_start().len()];
            let (body, tight) = comment_text(line);
            if body.is_empty() {
                return None;
            }
            if tight {
                return Some(format!("{pad}<!--{body}-->"));
            }
            Some(format!("{pad}<!-- {body} -->"))
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

    /// Attributes printed inside the tag, all of them with the placeholder, as
    /// SoapUI does: an attribute the user does not need is simply removed.
    fn inline_attributes(&self, field: &Field) -> String {
        let mut text = String::new();
        for child in fields_of(&field.kind) {
            if !child.attribute {
                continue;
            }
            text.push_str(&format!(
                " {}=\"{PLACEHOLDER}\"",
                self.qualified(&child.uri, &child.local)
            ));
        }
        text
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
    /// Markup of one field, with nothing commented yet and no cardinality note.
    fn markup(&self, field: &Field, indent: usize) -> Vec<String> {
        let pad = "  ".repeat(indent);
        let tag = self.qualified(&field.uri, &field.local);
        let mut lines = Vec::new();

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
            Kind::Leaf => {
                lines.push(format!("{pad}<{tag}{attributes}>{PLACEHOLDER}</{tag}>"));
            }
            Kind::Composite { fields } => {
                let children: Vec<String> = fields
                    .iter()
                    .filter(|child| !child.attribute)
                    .flat_map(|child| self.entry(child, indent + 1))
                    .collect();
                if children.is_empty() {
                    // Only attributes here: a type derived from a simple one by
                    // `simpleContent` has attributes *and* a value to write.
                    if field.text {
                        lines.push(format!("{pad}<{tag}{attributes}>{PLACEHOLDER}</{tag}>"));
                    } else {
                        lines.push(format!("{pad}<{tag}{attributes}></{tag}>"));
                    }
                } else {
                    lines.push(format!("{pad}<{tag}{attributes}>"));
                    lines.extend(children);
                    lines.push(format!("{pad}</{tag}>"));
                }
            }
            Kind::Choice { branches } => {
                for branch in branches {
                    lines.push(marker(&pad, CHOICE_NOTE));
                    lines.extend(comment_block(&self.entry(branch, indent)));
                }
            }
        }

        lines
    }

    /// Markup of one field with SoapUI's cardinality markers: an optional element
    /// stays in the template under `<!--Optional:-->` so the user deletes the tag
    /// they do not need, an unbounded one under a repetitions marker with a single
    /// copy to duplicate, and a choice prints commented alternatives. A finite
    /// `maxOccurs` needs no marker: it simply shows a couple of copies.
    fn entry(&self, field: &Field, indent: usize) -> Vec<String> {
        let pad = "  ".repeat(indent);

        if field.occurs.max == UNBOUNDED {
            let mut lines = vec![marker(
                &pad,
                if field.occurs.min == 0 {
                    ZERO_OR_MORE
                } else {
                    ONE_OR_MORE
                },
            )];
            lines.extend(self.markup(field, indent));
            return lines;
        }

        if field.occurs.max > 1 {
            let copies = (field.occurs.max as usize).min(REPETITIONS);
            let mut lines = Vec::new();
            for _ in 0..copies {
                lines.extend(self.markup(field, indent));
            }
            return lines;
        }

        if field.occurs.min == 0 {
            let mut lines = vec![marker(&pad, OPTIONAL_NOTE)];
            lines.extend(self.markup(field, indent));
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
        text: found.text,
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

    /// A chain of optional elements across two schemas, each also written
    /// `nillable="true"` because real WSDLs do that, plus a `choice` whose branch
    /// is a complex type — the shape that makes a marker of a child reach a
    /// commented branch of its parent. `nillable` itself adds no marker: the
    /// optional marker already says what the element means.
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
            <xsd:element name="OrderRef" type="ns1:OrderRefType" minOccurs="0"/>
          </xsd:choice>
          <xsd:element name="RequestType" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="OrderRefType">
        <xsd:sequence>
          <xsd:element name="Position" type="xsd:int"/>
          <xsd:element name="Note" type="xsd:string" minOccurs="0"/>
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

    fn template_for(source: &str, uri: &str, local: &str) -> String {
        let parsed = parse_wsdl(source).expect("wsdl");
        let index = XsdIndex::build(&parsed.dom);
        body_template(&parsed.dom, &index, parsed.dom.root(), uri, local).xml
    }

    fn template_of(source: &str) -> String {
        template_for(source, "http://nemo-ibe.com/Avia", "CloseSession")
    }

    #[test]
    fn comment_body_drops_the_markers_of_an_already_commented_line() {
        assert_eq!(comment_body("<!--Optional:-->"), "Optional:");
        assert_eq!(comment_body("<!-- <!--Optional:--> -->"), "Optional:");
        assert_eq!(
            comment_body("  <ns1>Login>?</ns1>Login>  "),
            "<ns1>Login>?</ns1>Login>"
        );
        // An unclosed opener is text rather than a comment; the markers still go.
        assert_eq!(comment_body("<!-- a -- b"), "a - - b");
    }

    #[test]
    fn comment_block_never_emits_an_empty_or_nested_comment() {
        let lines = vec![
            "   ".to_string(),
            "<!-- <!--Optional:--> -->".to_string(),
            "<a>x</a>".to_string(),
        ];
        assert_eq!(
            comment_block(&lines),
            vec![
                "<!--Optional:-->".to_string(),
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
        // A marker is a comment already: it is kept tight, not wrapped again.
        assert_eq!(
            comment_block(&["  <!--Zero or more repetitions:-->".to_string()]),
            vec!["  <!--Zero or more repetitions:-->".to_string()]
        );
    }

    #[test]
    fn a_note_survives_a_name_with_double_hyphens() {
        assert_eq!(
            note("  ", "cycle in ns:a--b"),
            "  <!-- cycle in ns:a- -b -->"
        );
        assert_eq!(note("", "unknown type a--b"), "<!-- unknown type a- -b -->");
        assert_eq!(marker("  ", "Optional:"), "  <!--Optional:-->");
        assert_eq!(marker("", "a--b"), "<!--a- -b-->");
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
        // A commented `choice` branch of a complex type keeps every line's own
        // indentation, so the block nests the way the schema does once the user
        // uncomments it.
        assert!(xml.contains("    <!-- <ns1:OrderRef> -->"), "{xml}");
        assert!(
            xml.contains("      <!-- <ns1:Position>?</ns1:Position> -->"),
            "{xml}"
        );
        assert!(xml.contains("      <!--Optional:-->"), "{xml}");
        assert!(
            xml.contains("      <!-- <ns1:Note>?</ns1:Note> -->"),
            "{xml}"
        );
        assert!(xml.contains("    <!-- </ns1:OrderRef> -->"), "{xml}");
    }

    #[test]
    fn prints_optional_elements_live_under_the_optional_marker() {
        let xml = template_of(SAMPLE);
        assert!(xml.contains("<tns:CloseSession xmlns:tns="), "{xml}");
        // Nothing is commented out: the marker tells the user the tag may go.
        assert!(xml.contains("  <!--Optional:-->\n  <tns:Request>"), "{xml}");
        assert!(
            xml.contains("        <!--Optional:-->\n        <ns1:SessionId>?</ns1:SessionId>"),
            "{xml}"
        );
        assert!(!xml.contains("<!-- <ns1:SessionId>"), "{xml}");
        // A `nillable` attribute adds nothing: one marker says it all.
        assert!(!xml.contains("nillable"), "{xml}");
        // A marker is never wrapped in another comment.
        assert!(!xml.contains("<!-- Optional: -->"), "{xml}");
        assert!(!xml.contains("<!--Optional:--> -->"), "{xml}");
    }

    #[test]
    fn prints_placeholder_values_instead_of_guessed_samples() {
        let xml = template_of(SAMPLE);
        assert!(
            xml.contains("<ns1:RequestType>?</ns1:RequestType>"),
            "{xml}"
        );
        // Nothing that looks like real data: the user replaces or deletes `?`.
        assert!(!xml.contains("text"), "{xml}");
        assert!(!xml.contains("2026"), "{xml}");
        assert!(!xml.contains(">0<"), "{xml}");
    }

    #[test]
    fn flattens_the_markers_of_a_choice_branch() {
        let xml = template_of(SAMPLE);
        assert_eq!(xml.matches("<!--Choice:-->").count(), 2, "{xml}");
        assert_eq!(
            xml.matches("<!-- <ns1:TicketRef>?</ns1:TicketRef> -->")
                .count(),
            1,
            "{xml}"
        );
        // The optional marker of a branch stays tight inside the commented block.
        assert!(!xml.contains("<!-- Optional: -->"), "{xml}");
        assert!(!xml.contains("nillable"), "{xml}");
        assert!(!xml.contains("<!-- <!--"), "{xml}");
        assert!(!xml.contains(" --> -->"), "{xml}");
    }

    /// Cardinality: SoapUI says what may be missing and how often a block may
    /// repeat, and the user copies or deletes tags accordingly.
    const OCCURS: &str = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
      xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
      xmlns:xsd="http://www.w3.org/2001/XMLSchema"
      xmlns:tns="http://example.com/cards"
      targetNamespace="http://example.com/cards">
  <wsdl:types>
    <xsd:schema targetNamespace="http://example.com/cards" elementFormDefault="qualified">
      <xsd:element name="Cards" type="tns:CardsRequest"/>
      <xsd:complexType name="CardsRequest">
        <xsd:sequence>
          <xsd:element name="Required" type="xsd:string"/>
          <xsd:element name="Optional" type="xsd:string" minOccurs="0"/>
          <xsd:element name="Any" type="tns:Card" minOccurs="0" maxOccurs="unbounded"/>
          <xsd:element name="AtLeastOne" type="tns:Card" maxOccurs="unbounded"/>
          <xsd:element name="UpToThree" type="tns:Card" maxOccurs="3"/>
        </xsd:sequence>
        <xsd:attribute name="Note" type="xsd:string"/>
      </xsd:complexType>
      <xsd:complexType name="Card">
        <xsd:sequence>
          <xsd:element name="Code" type="xsd:string"/>
        </xsd:sequence>
        <xsd:attribute name="Kind" type="xsd:string" use="required"/>
      </xsd:complexType>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="CardsInput">
    <wsdl:part name="body" element="tns:Cards"/>
  </wsdl:message>
  <wsdl:portType name="CardsPortType">
    <wsdl:operation name="Cards">
      <wsdl:input message="tns:CardsInput"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="CardsBinding" type="tns:CardsPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="Cards">
      <soap:operation soapAction="urn:Cards" style="document"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="CardsService">
    <wsdl:port name="CardsPort" binding="tns:CardsBinding">
      <soap:address location="http://127.0.0.1:9000/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    fn occurs_template() -> String {
        template_for(OCCURS, "http://example.com/cards", "Cards")
    }

    #[test]
    fn prints_the_repetitions_markers_of_a_repeating_element() {
        let xml = occurs_template();
        // An unbounded element appears once under a marker that says to copy it.
        assert!(
            xml.contains("  <!--Zero or more repetitions:-->\n  <tns:Any Kind=\"?\">"),
            "{xml}"
        );
        assert_eq!(xml.matches("<tns:Any").count(), 1, "{xml}");
        assert!(
            xml.contains("  <!--One or more repetitions:-->\n  <tns:AtLeastOne Kind=\"?\">"),
            "{xml}"
        );
        assert_eq!(xml.matches("<tns:AtLeastOne").count(), 1, "{xml}");
        // A finite `maxOccurs` shows two copies and needs no marker to copy them.
        assert!(xml.contains("  <tns:UpToThree Kind=\"?\">"), "{xml}");
        assert_eq!(xml.matches("<tns:UpToThree").count(), 2, "{xml}");
        assert!(!xml.contains("repetitions:-->\n  <tns:UpToThree"), "{xml}");
    }

    #[test]
    fn prints_every_attribute_with_the_placeholder() {
        let xml = occurs_template();
        // Optional attributes are printed too: an attribute the user does not
        // need is deleted along with the tag.
        assert!(xml.contains(r#"<tns:Cards Note="?"#), "{xml}");
        assert_eq!(xml.matches(r#"Kind="?"#).count(), 4, "{xml}");
        assert!(!xml.contains("optional attribute"), "{xml}");
        assert!(!xml.contains("<!--Note"), "{xml}");
    }

    #[test]
    fn a_required_leaf_carries_no_marker() {
        let xml = occurs_template();
        assert!(xml.contains("  <tns:Required>?</tns:Required>"), "{xml}");
        assert!(xml.contains("    <tns:Code>?</tns:Code>"), "{xml}");
        assert!(
            xml.contains("  <!--Optional:-->\n  <tns:Optional>?</tns:Optional>"),
            "{xml}"
        );
        build_dom(&xml).expect("the cardinality template is well-formed markup");
        for line in xml.lines() {
            assert!(line.matches("<!--").count() <= 1, "nested markers: {line}");
        }
    }

    /// A derived request type of the shape a real WSDL writes: a wrapper that
    /// extends a base requisites type and adds a body element of another
    /// namespace over it, plus a restriction and a `simpleContent` extension.
    const DERIVED: &str = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
      xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
      xmlns:xsd="http://www.w3.org/2001/XMLSchema"
      xmlns:tns="http://nemo-ibe.com/Avia"
      xmlns:ns1="http://nemo-ibe.com/STL"
      targetNamespace="http://nemo-ibe.com/Avia">
  <wsdl:types>
    <xsd:schema targetNamespace="http://nemo-ibe.com/Avia" elementFormDefault="qualified">
      <xsd:element name="Search">
        <xsd:complexType>
          <xsd:sequence>
            <xsd:element name="Request" type="ns1:RequestWithBody" minOccurs="0" nillable="true"/>
            <xsd:element name="Limited" type="ns1:LimitedRequest" minOccurs="0"/>
          </xsd:sequence>
        </xsd:complexType>
      </xsd:element>
    </xsd:schema>
    <xsd:schema targetNamespace="http://nemo-ibe.com/STL" elementFormDefault="qualified">
      <xsd:complexType name="BaseRequest">
        <xsd:sequence>
          <xsd:element name="Requisites" type="xsd:string" minOccurs="0"/>
          <xsd:element name="UserID" type="xsd:int"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="RequestWithBody">
        <xsd:complexContent>
          <xsd:extension base="ns1:BaseRequest">
            <xsd:sequence>
              <xsd:element name="RequestBody" type="ns1:SearchBody" nillable="true"/>
            </xsd:sequence>
          </xsd:extension>
        </xsd:complexContent>
      </xsd:complexType>
      <xsd:complexType name="SearchBody">
        <xsd:sequence>
          <xsd:element name="FlightRef" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="LimitedRequest">
        <xsd:complexContent>
          <xsd:restriction base="ns1:BaseRequest">
            <xsd:attribute name="Version" type="xsd:string"/>
          </xsd:restriction>
        </xsd:complexContent>
      </xsd:complexType>
      <xsd:complexType name="Amount">
        <xsd:simpleContent>
          <xsd:extension base="xsd:decimal">
            <xsd:attribute name="Currency" type="xsd:string" use="required"/>
          </xsd:extension>
        </xsd:simpleContent>
      </xsd:complexType>
      <xsd:element name="Total" type="ns1:Amount"/>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="SearchInput">
    <wsdl:part name="body" element="tns:Search"/>
  </wsdl:message>
  <wsdl:portType name="AviaPortType">
    <wsdl:operation name="Search">
      <wsdl:input message="tns:SearchInput"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="AviaBinding" type="tns:AviaPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="Search">
      <soap:operation soapAction="urn:Search" style="document"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="AviaService">
    <wsdl:port name="AviaPort" binding="tns:AviaBinding">
      <soap:address location="http://127.0.0.1:9000/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    fn derived_template() -> String {
        template_for(DERIVED, "http://nemo-ibe.com/Avia", "Search")
    }

    #[test]
    fn adds_the_elements_a_complex_extension_declares_over_its_base() {
        let xml = derived_template();
        // The base contributes its fields first, the extension then its own body.
        let requisites = xml.find("ns1:Requisites").expect("base field");
        let user_id = xml.find("ns1:UserID").expect("base field");
        let body = xml.find("ns1:RequestBody").expect("added element");
        assert!(requisites < user_id, "{xml}");
        assert!(user_id < body, "{xml}");
        // The added element keeps the namespace of the schema that declares it.
        assert!(
            xml.contains(r#"xmlns:ns1="http://nemo-ibe.com/STL""#),
            "{xml}"
        );
        assert!(xml.contains("<ns1:RequestBody>"), "{xml}");
        assert!(!xml.contains("nillable"), "{xml}");
        assert!(
            xml.find("ns1:FlightRef").expect("nested body field") > body,
            "{xml}"
        );
        assert!(
            xml.contains("      <!--Optional:-->\n      <ns1:FlightRef>?</ns1:FlightRef>"),
            "{xml}"
        );
    }

    #[test]
    fn keeps_the_base_of_a_restriction_and_the_attributes_it_adds() {
        let xml = derived_template();
        let limited = xml
            .find("<tns:Limited")
            .expect("the restricted element is printed");
        let block = &xml[limited..];
        // A restriction repeats what it allows, so the base fields stay visible.
        assert!(block.contains("ns1:Requisites"), "{block}");
        assert!(block.contains("ns1:UserID"), "{block}");
        // The attribute the derivation adds is printed on the tag, not in a note.
        assert!(block.contains("<tns:Limited Version=\"?\">"), "{block}");
        assert!(!xml.contains("optional attribute"), "{xml}");
    }

    #[test]
    fn prints_the_value_and_the_attributes_of_a_simple_extension() {
        let xml = template_for(DERIVED, "http://nemo-ibe.com/STL", "Total");
        assert_eq!(
            xml,
            r#"<ns1:Total Currency="?" xmlns:ns1="http://nemo-ibe.com/STL">?</ns1:Total>"#
        );
    }

    #[test]
    fn does_not_report_a_built_in_base_as_an_unknown_type() {
        let parsed = parse_wsdl(DERIVED).expect("wsdl");
        let index = XsdIndex::build(&parsed.dom);
        let out = body_template(
            &parsed.dom,
            &index,
            parsed.dom.root(),
            "http://nemo-ibe.com/STL",
            "Total",
        );
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    }

    #[test]
    fn prints_a_derived_template_that_reads_as_xml() {
        build_dom(&derived_template()).expect("template is well-formed markup");
        build_dom(&template_for(DERIVED, "http://nemo-ibe.com/STL", "Total"))
            .expect("template is well-formed markup");
    }

    #[test]
    fn prints_one_comment_per_line_for_a_derived_template() {
        for line in derived_template().lines() {
            assert!(line.matches("<!--").count() <= 1, "nested markers: {line}");
            assert!(line.matches("-->").count() <= 1, "nested markers: {line}");
            assert!(!line.contains("<!- -"), "softened opener: {line}");
            assert!(!line.contains("- ->"), "softened closer: {line}");
        }
    }
}
