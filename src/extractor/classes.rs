//! Extracción heurística de clases/métodos/atributos, en el mismo espíritu
//! que `extractor::imports`: texto por línea/bloque, no un parser real — se
//! rompe con macros, metaprogramación, o formato poco convencional. `Object`
//! queda fuera a propósito (una instancia en tiempo de ejecución no es algo
//! extraíble de forma estática de la misma manera).

use std::collections::HashMap;

pub struct ClassInfo {
    pub name: String,
    pub methods: Vec<String>,
    pub attributes: Vec<String>,
}

pub fn extract_classes(language: &str, content: &str) -> Vec<ClassInfo> {
    match language {
        "python" => extract_python_classes(content),
        "javascript" | "typescript" => extract_js_classes(content),
        "rust" => extract_rust_classes(content),
        "go" => extract_go_classes(content),
        "java" => extract_java_classes(content),
        "kotlin" => extract_kotlin_classes(content),
        "php" => extract_php_classes(content),
        "ruby" => extract_ruby_classes(content),
        "c" | "cplusplus" => extract_c_classes(content),
        "elixir" => extract_elixir_classes(content),
        "haskell" => extract_haskell_classes(content),
        "julia" => extract_julia_classes(content),
        "r" => extract_r_classes(content),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------
// Scanners compartidos de bajo nivel
// ---------------------------------------------------------------------

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// Línea (exclusiva) donde termina el cuerpo indentado que empieza justo
/// después de `start` — la primera línea no vacía con indentación <=
/// `header_indent`. Para Python/Ruby/Elixir: lenguajes que por convención
/// se indentan de forma consistente aunque Ruby/Elixir usen `end`.
fn indented_body_end(lines: &[&str], start: usize, header_indent: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        if indent_of(lines[i]) <= header_indent {
            break;
        }
        i += 1;
    }
    i
}

/// Enmascara TODO el archivo de una sola pasada (no línea por línea de
/// forma aislada) reemplazando por espacios el contenido de strings
/// (comillas simples/dobles, template literals), comentarios `//` y
/// comentarios de bloque `/* ... */` — estos últimos son la razón de
/// procesar el archivo entero de una vez: un doc-comment `/** ... */` de
/// varias líneas con paréntesis de prosa normal (muy común en este
/// código, en español) desincroniza el conteo de profundidad para el
/// resto del archivo si no se trackea el estado "dentro de un comentario
/// de bloque" entre líneas. El resultado tiene el mismo número de líneas
/// y el mismo largo por línea, así que los índices siguen siendo válidos.
/// No maneja strings/template literals que crucen varias líneas
/// (limitación heurística aceptada, como el resto del extractor).
fn mask_source(lines: &[&str]) -> Vec<String> {
    let mut result = Vec::with_capacity(lines.len());
    let mut in_block_comment = false;
    for line in lines {
        result.push(mask_line(line, &mut in_block_comment));
    }
    result
}

fn mask_line(line: &str, in_block_comment: &mut bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut result = String::with_capacity(chars.len());
    let mut in_string: Option<char> = None;
    let mut prev_significant: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if *in_block_comment {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                result.push(' ');
                result.push(' ');
                *in_block_comment = false;
                i += 2;
            } else {
                result.push(' ');
                i += 1;
            }
            continue;
        }
        if let Some(q) = in_string {
            if c == '\\' {
                result.push(' ');
                if i + 1 < chars.len() {
                    result.push(' ');
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            if c == q {
                in_string = None;
            }
            result.push(' ');
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            result.push(' ');
            result.push(' ');
            *in_block_comment = true;
            i += 2;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            for _ in i..chars.len() {
                result.push(' ');
            }
            break;
        }
        // Literal regex de JS/TS (`/.../flags`) — sin esto, paréntesis y
        // corchetes dentro del patrón (ej. `/\s*scale\([^)]*\)/`) desbalancean
        // el conteo ingenuo de braces/parens usado por el resto del extractor.
        // Heurística estándar: `/` inicia un regex salvo que el carácter
        // significativo previo sea el fin de un valor (identificador, `)`,
        // `]`, dígito) — en cuyo caso es división.
        if c == '/' {
            let starts_regex = match prev_significant {
                None => true,
                Some(pc) => !(pc.is_alphanumeric() || pc == '_' || pc == '$' || pc == ')' || pc == ']'),
            };
            if starts_regex {
                if let Some(end) = find_regex_literal_end(&chars, i) {
                    for _ in i..=end {
                        result.push(' ');
                    }
                    i = end + 1;
                    prev_significant = Some('/');
                    continue;
                }
            }
        }
        if c == '"' || c == '\'' || c == '`' {
            in_string = Some(c);
            result.push(' ');
            i += 1;
            continue;
        }
        result.push(c);
        if !c.is_whitespace() {
            prev_significant = Some(c);
        }
        i += 1;
    }
    result
}

/// Si el carácter `chars[start]` (un `/`) parece iniciar un literal regex de
/// JS/TS, devuelve el índice de su `/` de cierre en la misma línea (los
/// regex literales no pueden contener saltos de línea sin escapar). Respeta
/// escapes (`\`) y clases de caracteres (`[...]`, donde un `/` sin escapar
/// no cierra el regex).
fn find_regex_literal_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_class = false;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Línea donde cierra el bloque `{ ... }` que abre en o después de `start`
/// — balancea llaves ingenuamente (mismo criterio heurístico que el resto
/// del extractor). `lines` debe venir ya enmascarado (`mask_source`).
fn braced_body_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut opened = false;
    let mut i = start;
    while i < lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                depth += 1;
                opened = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if opened && depth <= 0 {
            return i;
        }
        i += 1;
    }
    lines.len().saturating_sub(1)
}

/// Índices de las líneas que están al nivel superior (profundidad 1) del
/// bloque `{ ... }` que abre en o después de `start` — se salta cualquier
/// bloque anidado (cuerpo de método, if, objeto literal, etc.) como una
/// unidad opaca, para no confundir su contenido con miembros de la clase.
///
/// También trackea profundidad de paréntesis: una firma de método partida
/// en varias líneas (parámetros multilínea, con o sin tipos objeto TS
/// entre llaves) no debe tratar cada línea de parámetro como un miembro
/// aparte — solo cuenta una línea si NO estamos en medio de un `(...)`
/// todavía sin cerrar de una línea anterior. `lines` debe venir ya
/// enmascarado (`mask_source`).
fn top_level_lines(lines: &[&str], start: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut opened = false;
    let mut i = start;
    while i < lines.len() {
        let brace_depth_before = brace_depth;
        let paren_depth_before = paren_depth;
        for ch in lines[i].chars() {
            match ch {
                '{' => {
                    brace_depth += 1;
                    opened = true;
                }
                '}' => brace_depth -= 1,
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                _ => {}
            }
        }
        if opened && i > start && brace_depth_before == 1 && paren_depth_before == 0 {
            result.push(i);
        }
        if opened && brace_depth <= 0 {
            break;
        }
        i += 1;
    }
    result
}

/// Encuentra el paréntesis que cierra el que está en `open_idx` (que debe
/// ser `(`), balanceando ingenuamente byte a byte.
fn find_matching_paren(content: &str, open_idx: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open_idx) {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|s| seen.insert(s.clone())).collect()
}

/// Si `s` es una asignación simple `nombre = valor` (no `==`/`!=`/`+=`/...),
/// devuelve `nombre`. No maneja tuplas/desestructuración.
fn simple_assignment_target(s: &str) -> Option<&str> {
    let s = s.trim();
    let eq = s.find('=')?;
    if s[eq..].starts_with("==") {
        return None;
    }
    if eq > 0 {
        let prev = s.as_bytes()[eq - 1];
        if matches!(prev, b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^') {
            return None;
        }
    }
    let name = s[..eq].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(name)
}

// ---------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------

fn extract_python_classes(content: &str) -> Vec<ClassInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = parse_python_class_name(rest) {
                let header_indent = indent_of(line);
                let body_end = indented_body_end(&lines, i, header_indent);
                let (methods, attributes) = scan_python_class_body(&lines[i + 1..body_end]);
                classes.push(ClassInfo { name, methods, attributes });
                i = body_end;
                continue;
            }
        }
        i += 1;
    }
    classes
}

fn parse_python_class_name(rest: &str) -> Option<String> {
    let end = rest.find(['(', ':']).unwrap_or(rest.len());
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn scan_python_class_body(body_lines: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    let body_base_indent = body_lines.iter().find(|l| !l.trim().is_empty()).map(|l| indent_of(l));
    for line in body_lines {
        if line.trim().is_empty() {
            continue;
        }
        let indent = indent_of(line);
        let trimmed = line.trim_start();
        if Some(indent) == body_base_indent {
            if let Some(rest) = trimmed.strip_prefix("async def ").or_else(|| trimmed.strip_prefix("def ")) {
                if let Some(name) = rest.split('(').next().map(str::trim).filter(|n| !n.is_empty()) {
                    methods.push(name.to_string());
                }
                continue;
            }
            if let Some(name) = simple_assignment_target(trimmed) {
                attributes.push(name.to_string());
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("self.") {
            if let Some(name) = simple_assignment_target(rest) {
                attributes.push(name.to_string());
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// JavaScript / TypeScript
// ---------------------------------------------------------------------

fn extract_js_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let after_export = trimmed.strip_prefix("export default ").or_else(|| trimmed.strip_prefix("export ")).unwrap_or(trimmed);
        if let Some(rest) = after_export.strip_prefix("class ") {
            if let Some(name) = rest.split([' ', '{', '<']).next().map(str::trim).filter(|n| !n.is_empty()) {
                let member_lines = top_level_lines(&lines, i);
                let (methods, attributes) = scan_js_members(&lines, &member_lines);
                classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                i = braced_body_end(&lines, i) + 1;
                continue;
            }
        }
        i += 1;
    }
    classes
}

fn scan_js_members(lines: &[&str], member_line_indices: &[usize]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        let core = trimmed
            .trim_start_matches("static ")
            .trim_start_matches("async ")
            .trim_start_matches("public ")
            .trim_start_matches("private ")
            .trim_start_matches("protected ")
            .trim_start_matches("readonly ")
            .trim_start_matches("override ")
            .trim_start_matches("get ")
            .trim_start_matches("set ")
            .trim_start_matches('*')
            .trim();
        if let Some(paren) = core.find('(') {
            let name = core[..paren].trim().trim_start_matches('#');
            if name == "constructor" {
                continue;
            }
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                methods.push(name.to_string());
                continue;
            }
        }
        let name_end = core.find([':', '=']).unwrap_or_else(|| core.trim_end_matches([';', ',']).len());
        let name = core[..name_end].trim().trim_start_matches('#');
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            attributes.push(name.to_string());
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------

fn extract_rust_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut struct_order: Vec<String> = Vec::new();
    let mut attrs_by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut methods_by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let stripped = strip_rust_visibility(trimmed);
        if let Some(rest) = stripped.strip_prefix("struct ") {
            if let Some(name) = parse_rust_type_name(rest) {
                if rest.contains('{') && !rest.trim_end().ends_with(';') {
                    let member_lines = top_level_lines(&lines, i);
                    let fields = scan_rust_fields(&lines, &member_lines);
                    attrs_by_name.entry(name.clone()).or_default().extend(fields);
                    if !struct_order.contains(&name) {
                        struct_order.push(name);
                    }
                    i = braced_body_end(&lines, i) + 1;
                    continue;
                }
                if !struct_order.contains(&name) {
                    struct_order.push(name.clone());
                }
                attrs_by_name.entry(name).or_default();
            }
        } else if let Some(rest) = stripped.strip_prefix("impl ") {
            let target = rest.rsplit(" for ").next().unwrap_or(rest);
            if let Some(name) = parse_rust_type_name(target) {
                if rest.contains('{') {
                    let member_lines = top_level_lines(&lines, i);
                    let methods = scan_rust_fn_names(&lines, &member_lines);
                    methods_by_name.entry(name).or_default().extend(methods);
                    i = braced_body_end(&lines, i) + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    struct_order
        .into_iter()
        .map(|name| ClassInfo {
            methods: dedupe_strings(methods_by_name.remove(&name).unwrap_or_default()),
            attributes: dedupe_strings(attrs_by_name.remove(&name).unwrap_or_default()),
            name,
        })
        .collect()
}

fn strip_rust_visibility(s: &str) -> &str {
    let s = s.strip_prefix("pub(crate) ").or_else(|| s.strip_prefix("pub(super) ")).unwrap_or(s);
    s.strip_prefix("pub ").unwrap_or(s)
}

fn parse_rust_type_name(rest: &str) -> Option<String> {
    let end = rest.find(['{', '(', '<', ';']).unwrap_or_else(|| rest.find(char::is_whitespace).unwrap_or(rest.len()));
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn scan_rust_fields(lines: &[&str], member_line_indices: &[usize]) -> Vec<String> {
    let mut fields = Vec::new();
    for &idx in member_line_indices {
        let trimmed = strip_rust_visibility(lines[idx].trim()).trim_end_matches(',');
        if let Some(colon) = trimmed.find(':') {
            let name = trimmed[..colon].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                fields.push(name.to_string());
            }
        }
    }
    fields
}

fn scan_rust_fn_names(lines: &[&str], member_line_indices: &[usize]) -> Vec<String> {
    let mut methods = Vec::new();
    for &idx in member_line_indices {
        let trimmed = strip_rust_visibility(lines[idx].trim());
        let trimmed = trimmed.strip_prefix("async ").unwrap_or(trimmed);
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            if let Some(name) = rest.split(['(', '<']).next().map(str::trim).filter(|n| !n.is_empty()) {
                methods.push(name.to_string());
            }
        }
    }
    methods
}

// ---------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------

fn extract_go_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut struct_order: Vec<String> = Vec::new();
    let mut attrs_by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut methods_by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = trimmed.strip_prefix("type ") {
            if let Some(name) = rest.split_whitespace().next() {
                let after_name = rest[name.len()..].trim_start();
                if let Some(struct_rest) = after_name.strip_prefix("struct") {
                    if struct_rest.contains('{') {
                        let member_lines = top_level_lines(&lines, i);
                        let fields = scan_go_fields(&lines, &member_lines);
                        attrs_by_name.entry(name.to_string()).or_default().extend(fields);
                        if !struct_order.contains(&name.to_string()) {
                            struct_order.push(name.to_string());
                        }
                        i = braced_body_end(&lines, i) + 1;
                        continue;
                    }
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("func ") {
            if let Some(recv_rest) = rest.strip_prefix('(') {
                if let Some(close) = recv_rest.find(')') {
                    let receiver = &recv_rest[..close];
                    let type_name = receiver.split_whitespace().last().unwrap_or("").trim_start_matches('*');
                    let after_recv = recv_rest[close + 1..].trim_start();
                    if let Some(name) = after_recv.split('(').next().map(str::trim).filter(|n| !n.is_empty()) {
                        if !type_name.is_empty() {
                            methods_by_name.entry(type_name.to_string()).or_default().push(name.to_string());
                        }
                    }
                }
            }
        }
        i += 1;
    }
    struct_order
        .into_iter()
        .map(|name| ClassInfo {
            methods: dedupe_strings(methods_by_name.remove(&name).unwrap_or_default()),
            attributes: dedupe_strings(attrs_by_name.remove(&name).unwrap_or_default()),
            name,
        })
        .collect()
}

fn scan_go_fields(lines: &[&str], member_line_indices: &[usize]) -> Vec<String> {
    let mut fields = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(name) = trimmed.split_whitespace().next() {
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                fields.push(name.to_string());
            }
        }
    }
    fields
}

// ---------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------

fn extract_java_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = strip_java_modifiers(lines[i].trim_start());
        if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = rest.split([' ', '{', '<']).next().map(str::trim).filter(|n| !n.is_empty()) {
                let member_lines = top_level_lines(&lines, i);
                let (methods, attributes) = scan_java_members(&lines, &member_lines, name);
                classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                i = braced_body_end(&lines, i) + 1;
                continue;
            }
        }
        i += 1;
    }
    classes
}

fn strip_java_modifiers(s: &str) -> &str {
    let mut s = s;
    loop {
        let next = s
            .strip_prefix("public ")
            .or_else(|| s.strip_prefix("private "))
            .or_else(|| s.strip_prefix("protected "))
            .or_else(|| s.strip_prefix("static "))
            .or_else(|| s.strip_prefix("final "))
            .or_else(|| s.strip_prefix("abstract "))
            .or_else(|| s.strip_prefix("synchronized "))
            .or_else(|| s.strip_prefix("transient "))
            .or_else(|| s.strip_prefix("volatile "))
            .or_else(|| s.strip_prefix("default "));
        match next {
            Some(n) => s = n,
            None => break,
        }
    }
    s.trim()
}

fn scan_java_members(lines: &[&str], member_line_indices: &[usize], class_name: &str) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") || trimmed.starts_with('@') {
            continue;
        }
        let core = strip_java_modifiers(trimmed);
        if let Some(paren) = core.find('(') {
            let before_paren = core[..paren].trim();
            if let Some(name) = before_paren.rsplit(|c: char| c.is_whitespace() || c == '<' || c == '>').find(|s| !s.is_empty()) {
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    // el constructor (nombrado igual que la clase) no cuenta
                    // como método propio, igual que "constructor"/"__construct"
                    // en JS/PHP.
                    if name != class_name {
                        methods.push(name.to_string());
                    }
                    continue;
                }
            }
        }
        let before_eq = core.split('=').next().unwrap_or(core).trim_end_matches(';').trim();
        if before_eq.split_whitespace().count() >= 2 {
            if let Some(name) = before_eq.rsplit(|c: char| c.is_whitespace()).find(|s| !s.is_empty()) {
                let name = name.trim_end_matches("[]");
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    attributes.push(name.to_string());
                }
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// Kotlin
// ---------------------------------------------------------------------

fn extract_kotlin_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = strip_kotlin_modifiers(lines[i].trim_start());
        let trimmed = trimmed.strip_prefix("data ").or_else(|| trimmed.strip_prefix("sealed ")).or_else(|| trimmed.strip_prefix("abstract ")).unwrap_or(trimmed);
        if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = rest.split(['(', ':', '{', '<', ' ']).next().map(str::trim).filter(|n| !n.is_empty()) {
                let mut attributes = parse_kotlin_primary_constructor(rest);
                let mut methods = Vec::new();
                if rest.contains('{') || content_has_brace_soon(&lines, i) {
                    let member_lines = top_level_lines(&lines, i);
                    let (m, a) = scan_kotlin_members(&lines, &member_lines);
                    methods.extend(m);
                    attributes.extend(a);
                    i = braced_body_end(&lines, i) + 1;
                    classes.push(ClassInfo {
                        name: name.to_string(),
                        methods: dedupe_strings(methods),
                        attributes: dedupe_strings(attributes),
                    });
                    continue;
                }
                classes.push(ClassInfo {
                    name: name.to_string(),
                    methods: dedupe_strings(methods),
                    attributes: dedupe_strings(attributes),
                });
            }
        }
        i += 1;
    }
    classes
}

/// `class Foo(...)` sin `{` en la misma línea puede tener el cuerpo en la
/// siguiente (`class Foo(...)\n{ ... }`, poco común pero válido). Evita
/// llamar a `top_level_lines`/`braced_body_end` innecesariamente si no hay
/// ninguna llave cerca.
fn content_has_brace_soon(lines: &[&str], start: usize) -> bool {
    lines.get(start + 1).map(|l| l.trim_start().starts_with('{')).unwrap_or(false)
}

fn strip_kotlin_modifiers(s: &str) -> &str {
    let mut s = s;
    loop {
        let next = s
            .strip_prefix("public ")
            .or_else(|| s.strip_prefix("private "))
            .or_else(|| s.strip_prefix("protected "))
            .or_else(|| s.strip_prefix("internal "))
            .or_else(|| s.strip_prefix("override "))
            .or_else(|| s.strip_prefix("open "))
            .or_else(|| s.strip_prefix("final "))
            .or_else(|| s.strip_prefix("suspend "))
            .or_else(|| s.strip_prefix("inline "))
            .or_else(|| s.strip_prefix("lateinit "));
        match next {
            Some(n) => s = n,
            None => break,
        }
    }
    s.trim()
}

fn scan_kotlin_members(lines: &[&str], member_line_indices: &[usize]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let core = strip_kotlin_modifiers(trimmed);
        if let Some(rest) = core.strip_prefix("fun ") {
            if let Some(name) = rest.split(['(', '<']).next().map(str::trim).filter(|n| !n.is_empty()) {
                methods.push(name.to_string());
                continue;
            }
        }
        if let Some(rest) = core.strip_prefix("val ").or_else(|| core.strip_prefix("var ")) {
            if let Some(name) = rest.split([':', '=']).next().map(str::trim).filter(|n| !n.is_empty()) {
                attributes.push(name.to_string());
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

fn parse_kotlin_primary_constructor(rest: &str) -> Vec<String> {
    let Some(open) = rest.find('(') else { return Vec::new() };
    let Some(close) = rest.rfind(')') else { return Vec::new() };
    if close <= open {
        return Vec::new();
    }
    rest[open + 1..close]
        .split(',')
        .filter_map(|param| {
            let param = param.trim();
            let param = param.strip_prefix("val ").or_else(|| param.strip_prefix("var "))?;
            param.split(':').next().map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
        })
        .collect()
}

// ---------------------------------------------------------------------
// PHP
// ---------------------------------------------------------------------

fn extract_php_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let trimmed = trimmed.strip_prefix("abstract ").or_else(|| trimmed.strip_prefix("final ")).unwrap_or(trimmed);
        if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = rest.split([' ', '{']).next().map(str::trim).filter(|n| !n.is_empty()) {
                let member_lines = top_level_lines(&lines, i);
                let (methods, attributes) = scan_php_members(&lines, &member_lines);
                classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                i = braced_body_end(&lines, i) + 1;
                continue;
            }
        }
        i += 1;
    }
    classes
}

fn strip_php_modifiers(s: &str) -> &str {
    let mut s = s;
    loop {
        let next = s
            .strip_prefix("public ")
            .or_else(|| s.strip_prefix("private "))
            .or_else(|| s.strip_prefix("protected "))
            .or_else(|| s.strip_prefix("static "))
            .or_else(|| s.strip_prefix("final "))
            .or_else(|| s.strip_prefix("abstract "))
            .or_else(|| s.strip_prefix("readonly "));
        match next {
            Some(n) => s = n,
            None => break,
        }
    }
    s.trim()
}

fn scan_php_members(lines: &[&str], member_line_indices: &[usize]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        let core = strip_php_modifiers(trimmed);
        if let Some(rest) = core.strip_prefix("function ") {
            let rest = rest.strip_prefix('&').unwrap_or(rest);
            if let Some(name) = rest.split('(').next().map(str::trim).filter(|n| !n.is_empty()) {
                if name != "__construct" {
                    methods.push(name.to_string());
                }
                continue;
            }
        }
        if let Some(dollar) = core.find('$') {
            let rest = core[dollar + 1..].trim_end_matches(';');
            if let Some(name) = simple_assignment_target(rest) {
                attributes.push(name.to_string());
            } else if rest.chars().all(|c| c.is_alphanumeric() || c == '_') && !rest.is_empty() {
                attributes.push(rest.to_string());
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// Ruby
// ---------------------------------------------------------------------

fn extract_ruby_classes(content: &str) -> Vec<ClassInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("class ") {
            let first_char_upper = rest.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            if first_char_upper {
                if let Some(name) = rest.split([' ', '<']).next().map(str::trim).filter(|n| !n.is_empty()) {
                    let header_indent = indent_of(line);
                    let body_end = indented_body_end(&lines, i, header_indent);
                    let (methods, attributes) = scan_ruby_class_body(&lines[i + 1..body_end]);
                    classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                    i = body_end;
                    continue;
                }
            }
        }
        i += 1;
    }
    classes
}

fn scan_ruby_class_body(body_lines: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    let body_base_indent = body_lines.iter().find(|l| !l.trim().is_empty()).map(|l| indent_of(l));
    for line in body_lines {
        if line.trim().is_empty() {
            continue;
        }
        let indent = indent_of(line);
        let trimmed = line.trim_start();
        if Some(indent) == body_base_indent {
            if let Some(rest) = trimmed.strip_prefix("def ") {
                let rest = rest.strip_prefix("self.").unwrap_or(rest);
                if let Some(name) = rest.split(['(', ' ']).next().filter(|n| !n.is_empty()) {
                    methods.push(name.to_string());
                }
                continue;
            }
            for macro_name in ["attr_accessor ", "attr_reader ", "attr_writer "] {
                if let Some(rest) = trimmed.strip_prefix(macro_name) {
                    for part in rest.split(',') {
                        let name = part.trim().trim_start_matches(':').trim_matches(|c| c == '"' || c == '\'');
                        if !name.is_empty() {
                            attributes.push(name.to_string());
                        }
                    }
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix('@') {
            if !rest.starts_with('@') {
                if let Some(name) = simple_assignment_target(rest) {
                    attributes.push(name.to_string());
                }
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// C / C++
// ---------------------------------------------------------------------

fn extract_c_classes(content: &str) -> Vec<ClassInfo> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let masked = mask_source(&raw_lines);
    let lines: Vec<&str> = masked.iter().map(String::as_str).collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let keyword = if trimmed.starts_with("class ") {
            Some(6)
        } else if trimmed.starts_with("struct ") {
            Some(7)
        } else {
            None
        };
        if let Some(skip) = keyword {
            let rest = &trimmed[skip..];
            if let Some(name) = rest.split([' ', ':', '{']).next().map(str::trim).filter(|n| !n.is_empty()) {
                if rest.contains('{') {
                    let member_lines = top_level_lines(&lines, i);
                    let (methods, attributes) = scan_c_members(&lines, &member_lines, name);
                    classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                    i = braced_body_end(&lines, i) + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    classes
}

fn scan_c_members(lines: &[&str], member_line_indices: &[usize], class_name: &str) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    for &idx in member_line_indices {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('*')
            || trimmed.starts_with("/*")
            || trimmed.starts_with("public:")
            || trimmed.starts_with("private:")
            || trimmed.starts_with("protected:")
        {
            continue;
        }
        let core = trimmed
            .trim_start_matches("virtual ")
            .trim_start_matches("static ")
            .trim_start_matches("inline ")
            .trim_start_matches("explicit ")
            .trim_start_matches("friend ");
        if let Some(paren) = core.find('(') {
            let before = core[..paren].trim();
            if let Some(name) = before.rsplit(|c: char| c.is_whitespace() || c == '*' || c == '&').find(|s| !s.is_empty()) {
                let name = name.trim_start_matches('~');
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    // constructor/destructor (nombrado igual que la clase) no
                    // cuenta como método propio.
                    if name != class_name {
                        methods.push(name.to_string());
                    }
                    continue;
                }
            }
        }
        let before_eq = core.split('=').next().unwrap_or(core).trim_end_matches(';').trim();
        if before_eq.split_whitespace().count() >= 2 {
            if let Some(name) = before_eq.rsplit(|c: char| c.is_whitespace() || c == '*' || c == '&').find(|s| !s.is_empty()) {
                let name = name.trim_end_matches("[]");
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    attributes.push(name.to_string());
                }
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// Elixir
// ---------------------------------------------------------------------

fn extract_elixir_classes(content: &str) -> Vec<ClassInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("defmodule ") {
            if let Some(name) = rest.split_whitespace().next().filter(|n| !n.is_empty()) {
                let header_indent = indent_of(line);
                let body_end = indented_body_end(&lines, i, header_indent);
                let (methods, attributes) = scan_elixir_module_body(&lines[i + 1..body_end]);
                classes.push(ClassInfo { name: name.to_string(), methods, attributes });
                i = body_end;
                continue;
            }
        }
        i += 1;
    }
    classes
}

fn scan_elixir_module_body(body_lines: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    let body_base_indent = body_lines.iter().find(|l| !l.trim().is_empty()).map(|l| indent_of(l));
    for line in body_lines {
        if line.trim().is_empty() || Some(indent_of(line)) != body_base_indent {
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("def ").or_else(|| trimmed.strip_prefix("defp ")) {
            if let Some(name) = rest.split(['(', ' ']).next().map(|n| n.trim_end_matches(',')).filter(|n| !n.is_empty()) {
                methods.push(name.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix('@') {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() && name.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
                attributes.push(name.to_string());
            }
        }
    }
    (dedupe_strings(methods), dedupe_strings(attributes))
}

// ---------------------------------------------------------------------
// Haskell
// ---------------------------------------------------------------------

fn extract_haskell_classes(content: &str) -> Vec<ClassInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("data ").or_else(|| trimmed.strip_prefix("newtype ")) {
            if let Some(name) = rest.split_whitespace().next().filter(|n| !n.is_empty()) {
                let attributes = find_haskell_record_fields(&lines, i);
                classes.push(ClassInfo { name: name.to_string(), methods: Vec::new(), attributes });
            }
        } else if let Some(rest) = trimmed.strip_prefix("class ") {
            let name_part = rest.split(" where").next().unwrap_or(rest);
            if let Some(name) = name_part.split_whitespace().next().filter(|n| !n.is_empty()) {
                let header_indent = indent_of(line);
                let body_end = indented_body_end(&lines, i, header_indent);
                let methods = scan_haskell_class_signatures(&lines[i + 1..body_end]);
                classes.push(ClassInfo { name: name.to_string(), methods, attributes: Vec::new() });
                i = body_end;
                continue;
            }
        }
        i += 1;
    }
    classes
}

/// Busca `{ campo :: Tipo, ... }` empezando en la línea `start` o en las
/// pocas líneas siguientes (records cortos de una o pocas líneas — el caso
/// común; uno muy largo/multilínea con formato inusual no se reconoce).
fn find_haskell_record_fields(lines: &[&str], start: usize) -> Vec<String> {
    let search_limit = (start + 4).min(lines.len());
    let Some(open_line) = (start..search_limit).find(|&li| lines[li].contains('{')) else {
        return Vec::new();
    };
    let close_line = braced_body_end(lines, open_line);
    let joined = lines[open_line..=close_line.min(lines.len().saturating_sub(1))].join(" ");
    let (Some(open), Some(close)) = (joined.find('{'), joined.rfind('}')) else {
        return Vec::new();
    };
    if close <= open {
        return Vec::new();
    }
    dedupe_strings(
        joined[open + 1..close]
            .split(',')
            .filter_map(|part| part.split("::").next().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string))
            .collect(),
    )
}

fn scan_haskell_class_signatures(body_lines: &[&str]) -> Vec<String> {
    let mut methods = Vec::new();
    for line in body_lines {
        if line.trim().is_empty() {
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(name) = trimmed.split("::").next().map(str::trim).filter(|n| !n.is_empty() && !n.contains(' ')) {
            methods.push(name.to_string());
        }
    }
    dedupe_strings(methods)
}

// ---------------------------------------------------------------------
// Julia
// ---------------------------------------------------------------------

fn extract_julia_classes(content: &str) -> Vec<ClassInfo> {
    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let rest = trimmed.strip_prefix("mutable struct ").or_else(|| trimmed.strip_prefix("struct "));
        if let Some(rest) = rest {
            if let Some(name) = rest.split([' ', '<']).next().filter(|n| !n.is_empty()) {
                let header_indent = indent_of(line);
                let end = find_julia_end(&lines, i, header_indent);
                let attributes = scan_julia_struct_fields(&lines[i + 1..end]);
                classes.push(ClassInfo { name: name.to_string(), methods: Vec::new(), attributes });
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    classes
}

/// Julia no es sensible a indentación, pero el estilo convencional sí
/// indenta el cuerpo — se busca la primera línea que sea exactamente `end`
/// a una indentación <= la de la cabecera.
fn find_julia_end(lines: &[&str], start: usize, header_indent: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim() == "end" && indent_of(lines[i]) <= header_indent {
            return i;
        }
        i += 1;
    }
    lines.len().saturating_sub(1)
}

fn scan_julia_struct_fields(body_lines: &[&str]) -> Vec<String> {
    let mut fields = Vec::new();
    for line in body_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let name = trimmed.split("::").next().unwrap_or(trimmed).trim();
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            fields.push(name.to_string());
        }
    }
    dedupe_strings(fields)
}

// ---------------------------------------------------------------------
// R (S4 vía setClass/representation, R6 vía R6Class) — el más frágil de
// los 13: son dos sistemas de OOP distintos, y R6 requiere parsear
// `list(...)` anidado a mano.
// ---------------------------------------------------------------------

fn extract_r_classes(content: &str) -> Vec<ClassInfo> {
    let mut classes = extract_r_s4_classes(content);
    classes.extend(extract_r6_classes(content));
    classes
}

fn extract_r_s4_classes(content: &str) -> Vec<ClassInfo> {
    let mut classes = Vec::new();
    let needle = "setClass(";
    let mut search_from = 0;
    while let Some(rel) = content[search_from..].find(needle) {
        let open_idx = search_from + rel + needle.len() - 1;
        let Some(close_idx) = find_matching_paren(content, open_idx) else { break };
        let inner = &content[open_idx + 1..close_idx];
        if let Some(name) = extract_quoted_first_arg(inner) {
            let attributes = parse_r_representation_fields(inner);
            classes.push(ClassInfo { name, methods: Vec::new(), attributes });
        }
        search_from = close_idx + 1;
    }
    classes
}

fn extract_r6_classes(content: &str) -> Vec<ClassInfo> {
    let mut classes = Vec::new();
    let needle = "R6Class(";
    let mut search_from = 0;
    while let Some(rel) = content[search_from..].find(needle) {
        let open_idx = search_from + rel + needle.len() - 1;
        let Some(close_idx) = find_matching_paren(content, open_idx) else { break };
        let inner = &content[open_idx + 1..close_idx];
        if let Some(name) = extract_quoted_first_arg(inner) {
            let mut methods = Vec::new();
            let mut attributes = Vec::new();
            for section in ["public = list(", "private = list(", "active = list("] {
                if let Some(rel2) = inner.find(section) {
                    let list_open = rel2 + section.len() - 1;
                    if let Some(list_close) = find_matching_paren(inner, list_open) {
                        let (m, a) = parse_r6_list_members(&inner[list_open + 1..list_close]);
                        methods.extend(m);
                        attributes.extend(a);
                    }
                }
            }
            classes.push(ClassInfo { name, methods: dedupe_strings(methods), attributes: dedupe_strings(attributes) });
        }
        search_from = close_idx + 1;
    }
    classes
}

fn extract_quoted_first_arg(inner: &str) -> Option<String> {
    let trimmed = inner.trim_start();
    let quote = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &trimmed[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn parse_r_representation_fields(inner: &str) -> Vec<String> {
    let Some(rep_rel) = inner.find("representation(") else { return Vec::new() };
    let open_idx = rep_rel + "representation(".len() - 1;
    let Some(close_idx) = find_matching_paren(inner, open_idx) else { return Vec::new() };
    dedupe_strings(
        inner[open_idx + 1..close_idx]
            .split(',')
            .filter_map(|part| part.split('=').next().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string))
            .collect(),
    )
}

/// Separa `body` por comas de nivel superior (ingenuo: cuenta profundidad
/// de paréntesis) y clasifica cada `nombre = valor` como método si el
/// valor empieza con `function`, si no como atributo.
fn parse_r6_list_members(body: &str) -> (Vec<String>, Vec<String>) {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut depth = 0i32;
    let mut current_start = 0usize;
    let mut parts = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(chars[current_start..i].iter().collect::<String>());
                current_start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(chars[current_start..].iter().collect::<String>());

    for part in parts {
        let part = part.trim();
        let Some(eq) = part.find('=') else { continue };
        let name = part[..eq].trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let value = part[eq + 1..].trim();
        if value.starts_with("function") {
            methods.push(name.to_string());
        } else {
            attributes.push(name.to_string());
        }
    }
    (methods, attributes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(classes: &[ClassInfo]) -> Vec<&str> {
        classes.iter().map(|c| c.name.as_str()).collect()
    }


    // --- Python ---

    #[test]
    fn extracts_python_class_with_methods_and_attributes() {
        let content = "class Foo(Base):\n    x = 1\n\n    def __init__(self, y):\n        self.y = y\n        self.z = y + 1\n\n    def bar(self):\n        return self.y\n";
        let classes = extract_classes("python", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["__init__", "bar"]);
        assert_eq!(classes[0].attributes, vec!["x", "y", "z"]);
    }

    #[test]
    fn python_class_ends_at_dedent() {
        let content = "class Foo:\n    def a(self):\n        pass\n\ndef top_level():\n    pass\n";
        let classes = extract_classes("python", content);
        assert_eq!(classes[0].methods, vec!["a"]);
    }

    // --- JS/TS ---

    #[test]
    fn extracts_js_class_members() {
        let content = "class Foo extends Bar {\n  x = 1;\n  constructor() {\n    this.y = 2;\n  }\n  bar(a) {\n    const local = 1;\n    return local;\n  }\n  async baz() {}\n}\n";
        let classes = extract_classes("javascript", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["bar", "baz"]);
        assert_eq!(classes[0].attributes, vec!["x"]);
    }

    #[test]
    fn extracts_ts_class_with_typed_fields() {
        let content = "export class Foo {\n  private count: number = 0;\n  name: string;\n  greet(): string {\n    return this.name;\n  }\n}\n";
        let classes = extract_classes("typescript", content);
        assert_eq!(classes[0].attributes, vec!["count", "name"]);
        assert_eq!(classes[0].methods, vec!["greet"]);
    }

    #[test]
    fn js_nested_method_body_does_not_leak_into_attributes() {
        let content = "class Foo {\n  bar() {\n    let localVar = 1;\n    if (localVar) {\n      let another = 2;\n    }\n  }\n}\n";
        let classes = extract_classes("javascript", content);
        assert_eq!(classes[0].methods, vec!["bar"]);
        assert!(classes[0].attributes.is_empty());
    }

    #[test]
    fn ts_multiline_method_signature_params_are_not_treated_as_attributes() {
        // Cada línea de parámetro (incluida una con tipo objeto TS entre
        // llaves) no debe colarse como atributo — es la misma firma que
        // rompía con `layoutLinear` real en client/src/render.ts.
        let content = "class Foo {\n  private layoutLinear(\n    rootHierarchy: Hierarchy<TreeNode>,\n    geomById: Map<string, BoxGeom>,\n    vectors: { depth: Vec2; sibling: Vec2 }\n  ): { positioned: Positioned[] } {\n    const x = 1;\n    return { positioned: [] };\n  }\n}\n";
        let classes = extract_classes("typescript", content);
        assert_eq!(classes[0].methods, vec!["layoutLinear"]);
        assert!(classes[0].attributes.is_empty(), "atributos inesperados: {:?}", classes[0].attributes);
    }

    // --- Rust ---

    #[test]
    fn extracts_rust_struct_and_impl() {
        let content = "pub struct Foo {\n    pub x: i32,\n    y: String,\n}\n\nimpl Foo {\n    pub fn new() -> Self { Self { x: 0, y: String::new() } }\n    fn bar(&self) -> i32 { self.x }\n}\n";
        let classes = extract_classes("rust", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["x", "y"]);
        assert_eq!(classes[0].methods, vec!["new", "bar"]);
    }

    #[test]
    fn rust_impl_trait_for_associates_by_target_name() {
        let content = "struct Foo { x: i32 }\n\nimpl std::fmt::Display for Foo {\n    fn fmt(&self) {}\n}\n";
        let classes = extract_classes("rust", content);
        assert_eq!(classes[0].methods, vec!["fmt"]);
    }

    // --- Go ---

    #[test]
    fn extracts_go_struct_and_receiver_methods() {
        let content = "type Foo struct {\n\tName string\n\tAge int\n}\n\nfunc (f *Foo) Greet() string {\n\treturn f.Name\n}\n\nfunc (f Foo) Old() bool {\n\treturn f.Age > 18\n}\n";
        let classes = extract_classes("go", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["Name", "Age"]);
        assert_eq!(classes[0].methods, vec!["Greet", "Old"]);
    }

    // --- Java ---

    #[test]
    fn extracts_java_class_members() {
        let content = "public class Foo {\n    private int count;\n    private String name = \"x\";\n\n    public Foo(String name) {\n        this.name = name;\n    }\n\n    public int getCount() {\n        return count;\n    }\n}\n";
        let classes = extract_classes("java", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["count", "name"]);
        assert_eq!(classes[0].methods, vec!["getCount"]);
    }

    // --- Kotlin ---

    #[test]
    fn extracts_kotlin_data_class_constructor_params() {
        let content = "data class Foo(val x: Int, var y: String)\n";
        let classes = extract_classes("kotlin", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["x", "y"]);
    }

    #[test]
    fn extracts_kotlin_regular_class_body() {
        let content = "class Foo {\n    val x: Int = 0\n    fun bar(): Int {\n        return x\n    }\n}\n";
        let classes = extract_classes("kotlin", content);
        assert_eq!(classes[0].attributes, vec!["x"]);
        assert_eq!(classes[0].methods, vec!["bar"]);
    }

    // --- PHP ---

    #[test]
    fn extracts_php_class_members() {
        let content = "class Foo extends Bar {\n    private $count = 0;\n    public $name;\n\n    public function __construct($name) {\n        $this->name = $name;\n    }\n\n    public function greet() {\n        return $this->name;\n    }\n}\n";
        let classes = extract_classes("php", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["count", "name"]);
        assert_eq!(classes[0].methods, vec!["greet"]);
    }

    // --- Ruby ---

    #[test]
    fn extracts_ruby_class_with_attr_accessor_and_methods() {
        let content = "class Foo < Base\n  attr_accessor :name, :age\n\n  def initialize(name)\n    @name = name\n    @active = true\n  end\n\n  def greet\n    @name\n  end\nend\n";
        let classes = extract_classes("ruby", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["initialize", "greet"]);
        assert_eq!(classes[0].attributes, vec!["name", "age", "active"]);
    }

    // --- C/C++ ---

    #[test]
    fn extracts_cpp_class_members() {
        let content = "class Foo {\npublic:\n    Foo();\n    int getX() const { return x; }\nprivate:\n    int x;\n    std::string name;\n};\n";
        let classes = extract_classes("cplusplus", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["getX"]);
        assert_eq!(classes[0].attributes, vec!["x", "name"]);
    }

    // --- Elixir ---

    #[test]
    fn extracts_elixir_module_with_attribute_and_defs() {
        let content = "defmodule Foo do\n  @default_name \"anon\"\n\n  def greet(name) do\n    name\n  end\n\n  defp helper do\n    :ok\n  end\nend\n";
        let classes = extract_classes("elixir", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["greet", "helper"]);
        assert_eq!(classes[0].attributes, vec!["default_name"]);
    }

    // --- Haskell ---

    #[test]
    fn extracts_haskell_record_fields() {
        let content = "data Foo = Foo { fooX :: Int, fooY :: String }\n";
        let classes = extract_classes("haskell", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["fooX", "fooY"]);
        assert!(classes[0].methods.is_empty());
    }

    #[test]
    fn extracts_haskell_type_class_signatures() {
        let content = "class Foo a where\n  bar :: a -> Int\n  baz :: a -> String\n";
        let classes = extract_classes("haskell", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].methods, vec!["bar", "baz"]);
    }

    // --- Julia ---

    #[test]
    fn extracts_julia_struct_fields_no_methods() {
        let content = "struct Foo\n    x::Int\n    y::String\nend\n";
        let classes = extract_classes("julia", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["x", "y"]);
        assert!(classes[0].methods.is_empty());
    }

    // --- R ---

    #[test]
    fn extracts_r_s4_class() {
        let content = "Foo <- setClass(\"Foo\", representation(x = \"numeric\", y = \"character\"))\n";
        let classes = extract_classes("r", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["x", "y"]);
    }

    #[test]
    fn extracts_r6_class_public_members() {
        let content = "Foo <- R6Class(\"Foo\", public = list(\n  x = NULL,\n  initialize = function(x) {\n    self$x <- x\n  },\n  greet = function() {\n    self$x\n  }\n))\n";
        let classes = extract_classes("r", content);
        assert_eq!(names(&classes), vec!["Foo"]);
        assert_eq!(classes[0].attributes, vec!["x"]);
        assert_eq!(classes[0].methods, vec!["initialize", "greet"]);
    }
}
