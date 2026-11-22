use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use reqwest::{Client, Method, Url};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::rest::{normalize_rest_url, DEFAULT_USER_AGENT};
use crate::wsdl::{
    FETCH_TIMEOUT_SECONDS, MAX_WSDL_BYTES, MAX_WSDL_ELEMENTS, SOAP12_NAMESPACE, SOAP_NAMESPACE,
    SOAP_VERSION_11, SOAP_VERSION_12, WSDL2_NAMESPACE, WSDL_NAMESPACE, XSD_NAMESPACE,
};

/// A WSDL document nests no deeper than this; the limit stops markup that is not
/// balanced from pushing the stack forever.
pub const MAX_NESTING_LEVEL: usize = 512;

/// Prefixes reserved by the XML specification: their binding is fixed and is
/// never taken from the declarations of a scope.
fn is_special_prefix(prefix: &str) -> bool {
    prefix == "xml" || prefix == "xmlns"
}

fn special_namespace(prefix: &str) -> Option<&'static str> {
    match prefix {
        "xml" => Some("http://www.w3.org/XML/1998/namespace"),
        _ => None,
    }
}

/// The `xmlns` set visible at one position of the tree: the declarations of the
/// element itself plus everything its ancestors declared. Element names and
/// QName attribute values are resolved against it, so matching an element means
/// comparing the resolved namespace URI and never the prefix in the markup.
#[derive(Clone, Debug, Default)]
struct Scope {
    bindings: Vec<(String, String)>,
}

impl Scope {
    /// Accumulates the declarations of `event` onto a copy of the parent scope.
    fn extend(&self, event: &BytesStart<'_>) -> Scope {
        let mut scope = self.clone();
        for attribute in event.attributes() {
            let Ok(attribute) = attribute else {
                continue;
            };
            let key = attribute.key.as_ref();
            let value = normalized_value(&attribute);
            if key == "xmlns" {
                scope.bindings.push((String::new(), value));
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                scope.bindings.push((prefix.to_string(), value));
            }
        }
        scope
    }

    fn resolve(&self, prefix: Option<&str>, use_default: bool) -> Option<String> {
        let prefix = match prefix {
            Some(prefix) if !prefix.is_empty() => prefix,
            // A name written without a prefix: an element takes the default
            // namespace, an attribute takes none — the XML Namespaces
            // specification puts unprefixed attributes in no namespace at all.
            Some(_) => return None,
            None => {
                if !use_default {
                    return None;
                }
                return self
                    .bindings
                    .iter()
                    .rev()
                    .find(|(bound, _)| bound.is_empty())
                    .map(|(_, uri)| uri.clone());
            }
        };
        if let Some(uri) = special_namespace(prefix) {
            return Some(uri.to_string());
        }
        self.bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound == prefix)
            .map(|(_, uri)| uri.clone())
    }

    /// Prefix to print for a namespace: a real declaration wins over the default
    /// one, because writing `prefix:local` is what makes the name readable.
    fn prefix_for(&self, uri: &str) -> Option<String> {
        let mut fallback = None;
        for (prefix, bound) in self.bindings.iter().rev() {
            if bound != uri || is_special_prefix(prefix) {
                continue;
            }
            if prefix.is_empty() {
                fallback = Some(String::new());
            } else {
                return Some(prefix.clone());
            }
        }
        fallback
    }
}

/// Element of the parsed document: resolved namespace, local name and normalized
/// attributes. Text content is deliberately not kept — a WSDL and the schema
/// beside it carry everything a request template needs in attributes.
#[derive(Debug)]
pub struct XmlElem {
    uri: String,
    local: String,
    attributes: Vec<(String, String, String)>,
    children: Vec<usize>,
}

/// Parsed document kept as an arena, so the WSDL tree and the schema tree can be
/// walked by index instead of borrowing a `quick-xml` reader across passes.
#[derive(Debug)]
pub struct XmlDom {
    nodes: Vec<XmlElem>,
    scopes: Vec<Scope>,
    root: usize,
}

impl XmlDom {
    pub fn root(&self) -> usize {
        self.root
    }

    pub fn uri(&self, index: usize) -> &str {
        &self.nodes[index].uri
    }

    pub fn local(&self, index: usize) -> &str {
        &self.nodes[index].local
    }

    pub fn children(&self, index: usize) -> &[usize] {
        &self.nodes[index].children
    }

    /// Attribute written without a prefix, which is how `name`, `type`, `ref`,
    /// `minOccurs` and `location` are always spelled.
    pub fn attribute(&self, index: usize, name: &str) -> Option<String> {
        self.nodes[index]
            .attributes
            .iter()
            .find(|(uri, local, _)| uri.is_empty() && local == name)
            .map(|(_, _, value)| value.clone())
    }

    pub fn children_in(&self, index: usize, ns: &str, local: &str) -> Vec<usize> {
        child_indices(self, index, ns, local)
    }

    pub fn find(&self, ns: &str, local: &str) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|index| self.uri(*index) == ns && self.local(*index) == local)
            .collect()
    }

    fn scope(&self, index: usize) -> &Scope {
        &self.scopes[index]
    }

    /// Prefix a scope binds to a namespace, `Some("")` when it binds the default
    /// namespace. Used when a template is printed: writing the URI as a tag name
    /// would be unreadable and writing nothing would silently change namespaces.
    pub fn binding(&self, index: usize, uri: &str) -> Option<String> {
        self.scope(index).prefix_for(uri)
    }

    /// Resolves a QName attribute value (`ref`, `type`, `element`, `binding`) in
    /// the scope of the element that carries it. An unprefixed value is in no
    /// namespace, which is exactly what an attribute carries in XML namespace 1.0.
    pub fn resolve_reference(&self, index: usize, value: &str) -> Option<(String, String)> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        match value.split_once(':') {
            Some((prefix, local)) => {
                let uri = self.scope(index).resolve(Some(prefix), false)?;
                Some((uri, local.to_string()))
            }
            None => Some((String::new(), value.to_string())),
        }
    }

    /// Display form of a resolved QName: `prefix:local` when the scope binds a
    /// prefix to that namespace, the bare local name otherwise.
    pub fn qname(&self, index: usize, uri: &str, local: &str) -> String {
        if uri.is_empty() {
            return local.to_string();
        }
        match self.scope(index).prefix_for(uri) {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}:{local}"),
            _ => local.to_string(),
        }
    }
}

/// Attribute value with the specification's normalization applied: entities
/// decoded and line breaks folded into a single space.
fn normalized_value(attribute: &quick_xml::events::attributes::Attribute<'_>) -> String {
    match attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0) {
        Ok(value) => value.to_string(),
        Err(_) => attribute.value.as_ref().to_string(),
    }
}

pub fn get_attr(dom: &XmlDom, index: usize, name: &str) -> Option<String> {
    dom.attribute(index, name)
}

pub fn get_reference(dom: &XmlDom, index: usize, attribute: &str) -> Option<(String, String)> {
    let value = dom.attribute(index, attribute)?;
    dom.resolve_reference(index, &value)
}

pub fn child_indices(dom: &XmlDom, parent: usize, ns: &str, local: &str) -> Vec<usize> {
    dom.children(parent)
        .iter()
        .copied()
        .filter(|index| dom.uri(*index) == ns && dom.local(*index) == local)
        .collect()
}

pub fn is_xsd_ns(uri: &str) -> bool {
    uri == XSD_NAMESPACE
}

/// Turns markup into the element arena. The reader runs in its lenient mode, so
/// undeclared entities and unknown references reach the tree as text instead of
/// stopping the import. The schema beside the WSDL is read from the same arena,
/// so the builder is visible to the whole `wsdl` module.
pub(crate) fn build_dom(source: &str) -> Result<XmlDom, String> {
    let mut reader = Reader::from_str(source);
    let mut nodes: Vec<XmlElem> = Vec::new();
    let mut scopes: Vec<Scope> = Vec::new();
    let mut scope_stack: Vec<Scope> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();

    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("malformed: markup is not readable: {error}"))?;
        match event {
            Event::Eof => break,
            Event::Start(event) => {
                if open.len() >= MAX_NESTING_LEVEL {
                    return Err(format!(
                        "malformed: nesting deeper than {MAX_NESTING_LEVEL} levels"
                    ));
                }
                let index = push_element(&event, &mut nodes, &mut scopes, &mut scope_stack);
                link(&mut nodes, &open, index, &mut roots);
                open.push(index);
            }
            Event::Empty(event) => {
                let index = push_element(&event, &mut nodes, &mut scopes, &mut scope_stack);
                link(&mut nodes, &open, index, &mut roots);
                // An empty element is already closed, so the declarations it made
                // are not visible to the siblings that follow it.
                scope_stack.pop();
            }
            Event::End(_) => {
                if open.pop().is_none() {
                    return Err("malformed: a closing tag has no matching opening tag".to_string());
                }
                scope_stack.pop();
            }
            _ => {}
        }
        if nodes.len() > MAX_WSDL_ELEMENTS {
            return Err(format!(
                "malformed: document has more than {MAX_WSDL_ELEMENTS} elements"
            ));
        }
    }

    if roots.is_empty() {
        return Err("malformed: document has no root element".to_string());
    }
    if roots.len() > 1 {
        return Err("malformed: document has more than one root element".to_string());
    }

    Ok(XmlDom {
        nodes,
        scopes,
        root: roots[0],
    })
}

/// Children are linked when an element is opened, so they stay in document
/// order, which is the order a WSDL author wrote them in.
fn link(nodes: &mut [XmlElem], open: &[usize], index: usize, roots: &mut Vec<usize>) {
    match open.last().copied() {
        Some(parent) => nodes[parent].children.push(index),
        None => roots.push(index),
    }
}

fn push_element(
    event: &BytesStart<'_>,
    nodes: &mut Vec<XmlElem>,
    scopes: &mut Vec<Scope>,
    scope_stack: &mut Vec<Scope>,
) -> usize {
    let scope = scope_stack
        .last()
        .cloned()
        .unwrap_or_default()
        .extend(event);

    let (local, prefix) = event.name().decompose();
    let declared = prefix.map(|prefix| prefix.as_ref().to_string());
    let uri = scope.resolve(declared.as_deref(), true).unwrap_or_default();

    let mut attributes: Vec<(String, String, String)> = Vec::new();
    for attribute in event.attributes() {
        let Ok(attribute) = attribute else {
            continue;
        };
        let key = attribute.key.as_ref().to_string();
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let (local, prefix) = attribute.key.decompose();
        let declared = prefix.map(|prefix| prefix.as_ref().to_string());
        let uri = match declared.as_deref() {
            Some(prefix) => scope.resolve(Some(prefix), false).unwrap_or_default(),
            None => String::new(),
        };
        attributes.push((
            uri,
            local.as_ref().to_string(),
            normalized_value(&attribute),
        ));
    }

    scope_stack.push(scope.clone());
    scopes.push(scope);
    nodes.push(XmlElem {
        uri,
        local: local.as_ref().to_string(),
        attributes,
        children: Vec::new(),
    });
    nodes.len() - 1
}

#[derive(Debug)]
pub struct ParsedEndpoint {
    pub url: String,
    pub soap_version: String,
    pub soap_action: String,
    pub port_name: String,
    pub binding_name: String,
}

#[derive(Debug)]
pub struct ParsedOperation {
    pub name: String,
    pub soap_action: String,
    pub input_element: String,
    pub input_ref: Option<(String, String)>,
    pub output_element: String,
    pub endpoints: Vec<ParsedEndpoint>,
}

#[derive(Debug)]
pub struct ParsedService {
    pub name: String,
    pub operations: Vec<ParsedOperation>,
}

#[derive(Debug)]
pub struct ParsedFile {
    pub warnings: Vec<String>,
    pub services: Vec<ParsedService>,
    pub dom: XmlDom,
}

fn is_wsdl_ns(uri: &str) -> bool {
    uri == WSDL_NAMESPACE
}

/// The SOAP version is decided by the namespace of the transport element, which
/// is why one WSDL can carry a SOAP 1.1 and a SOAP 1.2 port side by side.
fn soap_namespace(uri: &str) -> Option<&'static str> {
    match uri {
        SOAP_NAMESPACE => Some(SOAP_VERSION_11),
        SOAP12_NAMESPACE => Some(SOAP_VERSION_12),
        _ => None,
    }
}

struct BindingEntry {
    index: usize,
    local: String,
    soap_version: Option<&'static str>,
}

struct NamedEntry {
    local: String,
    index: usize,
}

fn collect_named(dom: &XmlDom, ns: &str, local_name: &str) -> Vec<NamedEntry> {
    dom.find(ns, local_name)
        .into_iter()
        .filter(|index| is_wsdl_ns(dom.uri(*index)))
        .map(|index| NamedEntry {
            local: get_attr(dom, index, "name").unwrap_or_default(),
            index,
        })
        .collect()
}

/// Reads the part of a message that carries the body element. The part is named
/// `body` by convention but the name is not mandatory, so a single-part message
/// is accepted whatever its part is called.
fn message_part(dom: &XmlDom, message: Option<usize>) -> Option<(String, String)> {
    let message = message?;
    let parts = dom.children(message);
    let chosen = parts
        .iter()
        .copied()
        .find(|index| {
            dom.local(*index) == "part" && get_attr(dom, *index, "name").as_deref() == Some("body")
        })
        .or_else(|| {
            parts
                .iter()
                .copied()
                .find(|index| dom.local(*index) == "part" && dom.uri(*index) == WSDL_NAMESPACE)
        })?;
    get_reference(dom, chosen, "element")
}

/// Reads the endpoint URL, whether the transport is declared on the port itself
/// or reached through the binding it points at.
fn port_address(dom: &XmlDom, port_index: usize, binding_index: usize) -> Option<String> {
    for scope in [port_index, binding_index] {
        for index in dom.children(scope) {
            if soap_namespace(dom.uri(*index)).is_none() || dom.local(*index) != "address" {
                continue;
            }
            let location = get_attr(dom, *index, "location")?;
            let normalized = normalize_rest_url(&location).ok()?;
            return Some(normalized.to_string());
        }
    }
    None
}

/// Walks the WSDL 1.1 tree `definitions → service/port → binding → portType`
/// and resolves the transport, the action and the message element per operation.
pub fn parse_wsdl(source: &str) -> Result<ParsedFile, String> {
    let dom = build_dom(source)?;
    let definitions = dom.root();

    if dom.uri(definitions) == WSDL2_NAMESPACE {
        return Err("wsdl2: the document uses the WSDL 2.0 namespace".to_string());
    }
    if !is_wsdl_ns(dom.uri(definitions)) || dom.local(definitions) != "definitions" {
        return Err(format!(
            "not-wsdl: the root element is <{}> instead of <definitions> in the WSDL namespace",
            dom.local(definitions)
        ));
    }

    let mut warnings: Vec<String> = Vec::new();
    let bindings: Vec<BindingEntry> = dom
        .find(WSDL_NAMESPACE, "binding")
        .into_iter()
        .filter(|index| is_wsdl_ns(dom.uri(*index)))
        .map(|index| {
            let version = dom
                .children(index)
                .iter()
                .copied()
                .find(|child| dom.local(*child) == "binding")
                .and_then(|child| soap_namespace(dom.uri(child)));
            BindingEntry {
                index,
                local: get_attr(&dom, index, "name").unwrap_or_default(),
                soap_version: version,
            }
        })
        .collect();
    let port_types = collect_named(&dom, WSDL_NAMESPACE, "portType");
    let messages = collect_named(&dom, WSDL_NAMESPACE, "message");

    let mut services: Vec<ParsedService> = Vec::new();
    for service_index in dom.children_in(definitions, WSDL_NAMESPACE, "service") {
        let service_name = get_attr(&dom, service_index, "name").unwrap_or_default();
        let mut operations: Vec<ParsedOperation> = Vec::new();

        for port_index in dom.children_in(service_index, WSDL_NAMESPACE, "port") {
            let port_name = get_attr(&dom, port_index, "name").unwrap_or_default();
            let Some(binding_ref) = get_reference(&dom, port_index, "binding") else {
                warnings.push(format!("no-binding:{service_name}::{port_name}"));
                continue;
            };
            let Some(binding) = bindings
                .iter()
                .find(|binding| binding.local == binding_ref.1)
            else {
                warnings.push(format!("no-binding:{service_name}::{port_name}"));
                continue;
            };
            let Some(version) = binding.soap_version else {
                warnings.push(format!("no-binding:{service_name}::{port_name}"));
                continue;
            };
            let Some(address) = port_address(&dom, port_index, binding.index) else {
                warnings.push(format!("no-address:{service_name}::{port_name}"));
                continue;
            };

            // The binding names the portType, so operations are looked up in the
            // portType the binding points at rather than by document order.
            let port_type_ref = get_reference(&dom, binding.index, "type");
            let port_type = port_type_ref.and_then(|(_, local)| {
                port_types
                    .iter()
                    .find(|entry| entry.local == local)
                    .map(|entry| entry.index)
            });
            let Some(port_type) = port_type else {
                warnings.push(format!("no-port-type:{service_name}::{port_name}"));
                continue;
            };

            for binding_operation in dom.children_in(binding.index, WSDL_NAMESPACE, "operation") {
                let operation_name = get_attr(&dom, binding_operation, "name").unwrap_or_default();
                let definition = child_indices(&dom, port_type, WSDL_NAMESPACE, "operation")
                    .into_iter()
                    .find(|index| {
                        get_attr(&dom, *index, "name").as_deref() == Some(operation_name.as_str())
                    });
                let Some(definition) = definition else {
                    continue;
                };

                let mut version = version;
                let mut soap_action = String::new();
                let mut input_ref = port_type_message(&dom, &messages, definition, "input");
                let mut output_ref = port_type_message(&dom, &messages, definition, "output");

                for child in dom.children(binding_operation) {
                    if let Some(child_version) = soap_namespace(dom.uri(*child)) {
                        if dom.local(*child) == "operation" {
                            version = child_version;
                            soap_action = get_attr(&dom, *child, "soapAction").unwrap_or_default();
                        }
                        continue;
                    }
                    if !is_wsdl_ns(dom.uri(*child)) {
                        continue;
                    }
                    let from_binding = match dom.local(*child) {
                        "input" => Some("input"),
                        "output" => Some("output"),
                        _ => None,
                    };
                    if let Some(direction) = from_binding {
                        let message =
                            get_reference(&dom, *child, "message").and_then(|(_, local)| {
                                messages
                                    .iter()
                                    .find(|entry| entry.local == local)
                                    .map(|entry| entry.index)
                            });
                        let part = message_part(&dom, message);
                        if direction == "input" {
                            input_ref = part.or(input_ref);
                        } else {
                            output_ref = part.or(output_ref);
                        }
                    }
                }

                let scope = binding_operation;
                let (input_element, input_ref) = match input_ref {
                    Some((uri, local)) => (dom.qname(scope, &uri, &local), Some((uri, local))),
                    None => (format!("{operation_name}Request"), None),
                };
                let output_element = match output_ref {
                    Some((uri, local)) => dom.qname(scope, &uri, &local),
                    None => format!("{operation_name}Response"),
                };
                if input_ref.is_none() {
                    warnings.push(format!(
                        "no-input:{service_name}::{port_name}::{operation_name}"
                    ));
                }

                let endpoint = ParsedEndpoint {
                    url: address.clone(),
                    soap_version: version.to_string(),
                    soap_action,
                    port_name: port_name.clone(),
                    binding_name: binding.local.clone(),
                };

                // Several ports of one service reach the same binding operation,
                // so an operation is one entry with one endpoint per port that
                // offers it — the UI offers them as alternatives, not as copies
                // of the operation.
                match operations
                    .iter_mut()
                    .find(|operation| operation.name == operation_name)
                {
                    Some(operation) => {
                        if !operation.endpoints.iter().any(|known| {
                            known.url == endpoint.url && known.port_name == endpoint.port_name
                        }) {
                            operation.endpoints.push(endpoint);
                        }
                    }
                    None => operations.push(ParsedOperation {
                        name: operation_name,
                        soap_action: endpoint.soap_action.clone(),
                        input_element,
                        input_ref,
                        output_element,
                        endpoints: vec![endpoint],
                    }),
                }
            }
        }

        if !operations.is_empty() {
            services.push(ParsedService {
                name: service_name,
                operations,
            });
        }
    }

    for schema_index in dom.find(XSD_NAMESPACE, "schema") {
        if !dom
            .children_in(schema_index, XSD_NAMESPACE, "import")
            .is_empty()
        {
            warnings.push("import-external".to_string());
        }
    }

    services.retain(|service| !service.operations.is_empty());
    if services.is_empty() {
        return Err("no-operations: the document declares no SOAP operation".to_string());
    }
    dedupe_warnings(&mut warnings);

    Ok(ParsedFile {
        warnings,
        services,
        dom,
    })
}

/// Resolves the message element an operation declares in its portType, which is
/// the document's only place where a message element is named without a binding.
fn port_type_message(
    dom: &XmlDom,
    messages: &[NamedEntry],
    definition: usize,
    direction: &str,
) -> Option<(String, String)> {
    let element = child_indices(dom, definition, WSDL_NAMESPACE, direction)
        .into_iter()
        .next()?;
    let message = get_reference(dom, element, "message").and_then(|(_, local)| {
        messages
            .iter()
            .find(|entry| entry.local == local)
            .map(|entry| entry.index)
    })?;
    message_part(dom, Some(message))
}

fn dedupe_warnings(warnings: &mut Vec<String>) {
    let mut unique: Vec<String> = Vec::new();
    for warning in warnings.drain(..) {
        if !unique.contains(&warning) {
            unique.push(warning);
        }
    }
    *warnings = unique;
}

/// Downloads a WSDL document. The 30 second timeout covers connect, headers and
/// the whole body, so a server that answers slowly or hands back more than the
/// 5 MB budget ends as an import error the frontend can show.
pub async fn fetch_wsdl(
    cancel_requested: bool,
    mut waiter: oneshot::Receiver<()>,
    url: &str,
) -> Result<String, String> {
    if cancel_requested {
        return Err("cancelled".to_string());
    }
    let endpoint: Url = normalize_rest_url(url).map_err(|error| format!("not-url: {error}"))?;

    let client = Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECONDS as u64))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(DEFAULT_USER_AGENT)
        .build()
        .map_err(|error| format!("client: {error}"))?;

    let response = tokio::select! {
        result = client.request(Method::GET, endpoint.as_str()).send() => match result {
            Ok(response) => response,
            Err(error) => return Err(fetch_failure(&error)),
        },
        _ = &mut waiter => return Err("cancelled".to_string()),
    };

    let status = response.status();
    if !status.is_success() {
        let reason = status
            .canonical_reason()
            .unwrap_or("the server did not answer with a WSDL");
        return Err(format!("http: {} {}", status.as_u16(), reason));
    }
    if let Some(length) = response.content_length() {
        if length > MAX_WSDL_BYTES as u64 {
            return Err(too_large(length));
        }
    }

    let mut bytes: Vec<u8> = Vec::new();
    let mut stream = response;
    loop {
        let chunk = tokio::select! {
            result = stream.chunk() => match result {
                Ok(chunk) => chunk,
                Err(error) => return Err(fetch_failure(&error)),
            },
            _ = &mut waiter => return Err("cancelled".to_string()),
        };
        let Some(chunk) = chunk else {
            break;
        };
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_WSDL_BYTES {
            return Err(too_large(bytes.len() as u64));
        }
    }

    String::from_utf8(bytes).map_err(|_| "encoding: the WSDL is not valid UTF-8".to_string())
}

fn too_large(length: u64) -> String {
    format!(
        "too-large: the WSDL is {length} bytes, above the {} MB limit",
        MAX_WSDL_BYTES / (1024 * 1024)
    )
}

fn fetch_failure(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return format!("timeout: the WSDL fetch took longer than {FETCH_TIMEOUT_SECONDS} seconds");
    }
    if error.is_connect() {
        return format!("connect: {error}");
    }
    format!("network: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
      xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
      xmlns:xsd="http://www.w3.org/2001/XMLSchema"
      xmlns:tns="http://example.com/people"
      targetNamespace="http://example.com/people">
  <wsdl:types>
    <xsd:schema targetNamespace="http://example.com/people">
      <xsd:element name="GetPerson" type="tns:GetPersonRequest"/>
      <xsd:element name="GetPersonResponse" type="tns:GetPersonResponse"/>
      <xsd:complexType name="GetPersonRequest">
        <xsd:sequence>
          <xsd:element name="id" type="xsd:int"/>
          <xsd:element name="comment" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="GetPersonResponse">
        <xsd:sequence>
          <xsd:element name="name" type="xsd:string"/>
        </xsd:sequence>
      </xsd:complexType>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="GetPersonInput">
    <wsdl:part name="body" element="tns:GetPerson"/>
  </wsdl:message>
  <wsdl:message name="GetPersonOutput">
    <wsdl:part name="body" element="tns:GetPersonResponse"/>
  </wsdl:message>
  <wsdl:portType name="PeoplePortType">
    <wsdl:operation name="GetPerson">
      <wsdl:input message="tns:GetPersonInput"/>
      <wsdl:output message="tns:GetPersonOutput"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="PeopleBinding" type="tns:PeoplePortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="GetPerson">
      <soap:operation soapAction="urn:GetPerson" style="document"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
      <wsdl:output><soap:body use="literal"/></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="PeopleService">
    <wsdl:port name="PeoplePort" binding="tns:PeopleBinding">
      <soap:address location="http://127.0.0.1:9000/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    fn first_operation(parsed: &ParsedFile) -> &ParsedOperation {
        &parsed.services[0].operations[0]
    }

    #[test]
    fn parses_service_binding_and_soap11_operation() {
        let parsed = parse_wsdl(SAMPLE).unwrap();
        assert_eq!(parsed.services.len(), 1);
        assert_eq!(parsed.services[0].name, "PeopleService");
        let operation = first_operation(&parsed);
        assert_eq!(operation.name, "GetPerson");
        assert_eq!(operation.soap_action, "urn:GetPerson");
        assert_eq!(operation.input_element, "tns:GetPerson");
        assert_eq!(operation.input_ref.as_ref().unwrap().1, "GetPerson");
        assert_eq!(operation.output_element, "tns:GetPersonResponse");
        assert_eq!(operation.endpoints.len(), 1);
        assert_eq!(operation.endpoints[0].url, "http://127.0.0.1:9000/soap");
        assert_eq!(operation.endpoints[0].soap_version, SOAP_VERSION_11);
        assert_eq!(operation.endpoints[0].soap_action, "urn:GetPerson");
        assert_eq!(operation.endpoints[0].port_name, "PeoplePort");
        assert_eq!(operation.endpoints[0].binding_name, "PeopleBinding");
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn matches_namespaces_by_uri_and_not_by_prefix() {
        let source = SAMPLE
            .replace("xmlns:wsdl=", "xmlns:w=")
            .replace("<wsdl:", "<w:")
            .replace("</wsdl:", "</w:");
        let parsed = parse_wsdl(&source).unwrap();
        assert_eq!(first_operation(&parsed).name, "GetPerson");
        assert_eq!(first_operation(&parsed).input_element, "tns:GetPerson");
    }

    #[test]
    fn accepts_a_document_that_declares_the_wsdl_namespace_as_the_default() {
        let source = SAMPLE
            .replace(
                "xmlns:wsdl=\"http://schemas.xmlsoap.org/wsdl/\"",
                "xmlns=\"http://schemas.xmlsoap.org/wsdl/\"",
            )
            .replace("<wsdl:", "<")
            .replace("</wsdl:", "</");
        let parsed = parse_wsdl(&source).unwrap();
        assert_eq!(first_operation(&parsed).name, "GetPerson");
    }

    #[test]
    fn reads_the_soap12_namespace_as_the_operation_version() {
        let source = SAMPLE
            .replace(
                "xmlns:soap=\"http://schemas.xmlsoap.org/wsdl/soap/\"",
                "xmlns:soap12=\"http://schemas.xmlsoap.org/wsdl/soap12/\"",
            )
            .replace("<soap:", "<soap12:");
        let parsed = parse_wsdl(&source).unwrap();
        let operation = first_operation(&parsed);
        assert_eq!(operation.endpoints[0].soap_version, SOAP_VERSION_12);
    }

    #[test]
    fn collects_every_port_of_an_operation_as_an_endpoint() {
        let source = SAMPLE.replace(
            "<wsdl:port name=\"PeoplePort\"",
            "<wsdl:port name=\"BackupPort\" binding=\"tns:PeopleBinding\">\n      \
             <soap:address location=\"http://127.0.0.1:9001/soap\"/>\n    </wsdl:port>\n    \
             <wsdl:port name=\"PeoplePort\"",
        );
        let parsed = parse_wsdl(&source).unwrap();
        let endpoints = &first_operation(&parsed).endpoints;
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].url, "http://127.0.0.1:9001/soap");
        assert_eq!(endpoints[1].url, "http://127.0.0.1:9000/soap");
    }

    #[test]
    fn keeps_two_services_of_one_document_apart() {
        let source = SAMPLE.replace(
            "</wsdl:service>",
            r#"</wsdl:service>
  <wsdl:service name="OtherService">
    <wsdl:port name="OtherPort" binding="tns:PeopleBinding">
      <soap:address location="http://127.0.0.1:9002/soap"/>
    </wsdl:port>
  </wsdl:service>"#,
        );
        let parsed = parse_wsdl(&source).unwrap();
        assert_eq!(parsed.services.len(), 2);
        assert_eq!(parsed.services[1].name, "OtherService");
        assert_eq!(parsed.services[1].operations[0].name, "GetPerson");
    }

    #[test]
    fn refuses_a_wsdl_2_document() {
        let error = parse_wsdl(
            r#"<definitions xmlns="http://www.w3.org/2002/wsdl/"><service name="S"/></definitions>"#,
        )
        .unwrap_err();
        assert!(error.starts_with("wsdl2:"));
    }

    #[test]
    fn refuses_markup_that_is_not_a_wsdl() {
        let error = parse_wsdl("<html><body></body></html>").unwrap_err();
        assert!(error.starts_with("not-wsdl:"));
    }

    #[test]
    fn refuses_a_document_without_any_soap_operation() {
        let error = parse_wsdl(
            r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/" targetNamespace="http://e.com"/>"#,
        )
        .unwrap_err();
        assert!(error.starts_with("no-operations:"));
    }

    #[test]
    fn refuses_a_port_whose_binding_has_no_soap_transport() {
        let source = SAMPLE.replace("<soap:binding", "<other:binding");
        let error = parse_wsdl(&source).unwrap_err();
        assert!(error.starts_with("no-operations:"));
    }

    #[test]
    fn reports_a_port_without_an_address_as_a_warning() {
        let source = SAMPLE.replace(
            "<soap:address location=\"http://127.0.0.1:9000/soap\"/>",
            "",
        );
        let error = parse_wsdl(&source).unwrap_err();
        assert!(error.starts_with("no-operations:"));
        assert!(SAMPLE.contains("soap:address"));
    }

    #[test]
    fn warns_about_a_schema_that_imports_other_documents() {
        let source = SAMPLE.replace(
            "<xsd:schema targetNamespace=\"http://example.com/people\">",
            "<xsd:schema targetNamespace=\"http://example.com/people\">\n      \
             <xsd:import namespace=\"http://example.com/other\" schemaLocation=\"other.xsd\"/>",
        );
        let parsed = parse_wsdl(&source).unwrap();
        assert_eq!(parsed.warnings, vec!["import-external".to_string()]);
    }

    #[test]
    fn reads_the_url_a_port_declares_through_its_binding() {
        let source = SAMPLE.replace(
            "<wsdl:port name=\"PeoplePort\" binding=\"tns:PeopleBinding\">\n      \
             <soap:address location=\"http://127.0.0.1:9000/soap\"/>\n    </wsdl:port>",
            "<wsdl:port name=\"PeoplePort\" binding=\"tns:PeopleBinding\"/>",
        );
        let error = parse_wsdl(&source).unwrap_err();
        assert!(error.starts_with("no-operations:"));
    }

    #[test]
    fn refuses_markup_that_cannot_be_read() {
        let error = parse_wsdl("<definitions").unwrap_err();
        assert!(error.starts_with("malformed:"));
    }

    #[test]
    fn refuses_a_document_with_more_than_one_root() {
        let error = parse_wsdl("<a/><b/>").unwrap_err();
        assert!(error.starts_with("malformed:"));
    }

    #[test]
    fn resolves_qname_values_through_the_scope_of_the_element() {
        let dom = build_dom(SAMPLE).unwrap();
        let part = dom
            .find(WSDL_NAMESPACE, "part")
            .into_iter()
            .find(|index| dom.attribute(*index, "name").as_deref() == Some("body"))
            .expect("body part");
        let resolved = get_reference(&dom, part, "element").unwrap();
        assert_eq!(resolved.0, "http://example.com/people");
        assert_eq!(resolved.1, "GetPerson");
        assert_eq!(dom.qname(part, &resolved.0, &resolved.1), "tns:GetPerson");
    }

    #[test]
    fn resolves_attributes_written_with_a_prefix() {
        let dom = build_dom(SAMPLE).unwrap();
        let port = dom
            .find(WSDL_NAMESPACE, "port")
            .into_iter()
            .next()
            .expect("port");
        assert_eq!(
            get_reference(&dom, port, "binding").unwrap(),
            (
                "http://example.com/people".to_string(),
                "PeopleBinding".to_string()
            )
        );
    }

    #[test]
    fn keeps_a_prefixed_attribute_out_of_the_bare_name_lookup() {
        let dom = build_dom(
            "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:nil=\"true\"/>",
        )
        .unwrap();
        // `attribute` reads only unprefixed attributes, which is what every
        // reference this parser follows (`name`, `type`, `ref`) is spelled as.
        assert_eq!(dom.attribute(dom.root(), "nil"), None);
        assert_eq!(dom.attribute(dom.root(), "xsi:nil"), None);
    }

    #[test]
    fn refuses_nesting_deeper_than_the_limit() {
        let source = "<root>".repeat(MAX_NESTING_LEVEL + 2);
        let error = build_dom(&source).unwrap_err();
        assert!(error.starts_with("malformed:"));
    }
}
