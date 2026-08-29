use serde_json::{json, Value};

#[derive(Debug)]
struct HtmlTemplate {
    plan: Value,
    expressions: Vec<DynamicExpression>,
    end: usize,
}

#[derive(Debug)]
struct DynamicExpression {
    source: String,
    context: DynamicContext,
}

#[derive(Debug, Clone, Copy)]
enum DynamicContext {
    Body,
    Attribute,
    Condition,
    Key,
}

pub fn rewrite_html_templates(source: &str) -> Result<(String, bool), String> {
    let mut out = String::with_capacity(source.len());
    let mut i = 0usize;
    let mut found = false;
    while i < source.len() {
        if let Some(name) = reserved_html_identifier_at(source, i) {
            if !is_html_runtime_source(source) || !is_reserved_html_declaration(source, i) {
                return Err(format_html_error(
                    source,
                    i,
                    HtmlError {
                        message: format!(
                            "`{name}` 是编译器保留的 HTML 内部标识符，不能在业务源码中使用"
                        ),
                        offset: i,
                    },
                ));
            }
        }
        if let Some(open) = html_open_at(source, i) {
            let parsed = Parser::new(source, open + 1)
                .parse_template()
                .map_err(|e| format_html_error(source, open, e))?;
            let plan = serde_json::to_string(&parsed.plan.to_string())
                .map_err(|e| format!("HTML 模板计划编码失败: {e}"))?;
            out.push_str("__qi_html模板(");
            out.push_str(&plan);
            out.push_str(", [");
            let wrapped = parsed
                .expressions
                .iter()
                .map(|e| match e.context {
                    DynamicContext::Body => format!("__qi_html正文值({})", e.source),
                    DynamicContext::Attribute => format!("__qi_html属性值({})", e.source),
                    DynamicContext::Condition => format!("__qi_html条件值({})", e.source),
                    DynamicContext::Key => format!("__qi_html键值({})", e.source),
                })
                .collect::<Vec<_>>();
            out.push_str(&wrapped.join(", "));
            out.push_str("])");
            i = parsed.end;
            found = true;
            continue;
        }

        let ch = source[i..].chars().next().unwrap();
        if ch == '"' || ch == '\'' || ch == '`' {
            let end = skip_quoted(source, i, ch)?;
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if source[i..].starts_with("//") {
            let end = source[i..]
                .find('\n')
                .map(|n| i + n)
                .unwrap_or(source.len());
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if source[i..].starts_with("/*") {
            let end = skip_block_comment(source, i)?;
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok((out, found))
}

fn is_html_runtime_source(source: &str) -> bool {
    let mut i = 0usize;
    loop {
        while i < source.len() {
            let ch = source[i..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        if source[i..].starts_with("//") {
            i = source[i..]
                .find('\n')
                .map(|n| i + n + 1)
                .unwrap_or(source.len());
            continue;
        }
        if source[i..].starts_with("/*") {
            i = match skip_block_comment(source, i) {
                Ok(end) => end,
                Err(_) => return false,
            };
            continue;
        }
        break;
    }
    source[i..].starts_with("包 Web.HTML块;")
}

fn is_reserved_html_declaration(source: &str, i: usize) -> bool {
    let prefix = source[..i].trim_end();
    prefix
        .strip_suffix("函数")
        .map(|before| {
            before
                .chars()
                .next_back()
                .map(|ch| !ch.is_alphanumeric() && ch != '_')
                .unwrap_or(true)
        })
        .unwrap_or(false)
}

fn reserved_html_identifier_at(source: &str, i: usize) -> Option<&'static str> {
    const RESERVED: [&str; 10] = [
        "__qi_html模板",
        "__qi_html正文值",
        "__qi_html属性值",
        "__qi_html条件值",
        "__qi_html键值",
        "__qi_html循环块",
        "__qi_html循环开始",
        "__qi_html循环加项",
        "__qi_html子块值",
        "__qi_html渲染块组",
    ];
    RESERVED.into_iter().find(|name| {
        if !source[i..].starts_with(name) {
            return false;
        }
        let before_ok = i == 0
            || source[..i]
                .chars()
                .next_back()
                .map(|ch| !ch.is_alphanumeric() && ch != '_')
                .unwrap_or(true);
        let end = i + name.len();
        let after_ok = end == source.len()
            || source[end..]
                .chars()
                .next()
                .map(|ch| !ch.is_alphanumeric() && ch != '_')
                .unwrap_or(true);
        before_ok && after_ok
    })
}

#[derive(Debug)]
struct HtmlError {
    message: String,
    offset: usize,
}

type HtmlResult<T> = Result<T, HtmlError>;

fn format_html_error(source: &str, fallback: usize, error: HtmlError) -> String {
    let offset = error.offset.min(source.len()).max(fallback);
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = source[line_start..offset].chars().count() + 1;
    format!("{}（第 {} 行第 {} 列）", error.message, line, column)
}

fn html_open_at(source: &str, i: usize) -> Option<usize> {
    if !source[i..].starts_with("HTML") {
        return None;
    }
    if i > 0 {
        let prev = source[..i].chars().next_back()?;
        if prev.is_alphanumeric() || prev == '_' {
            return None;
        }
    }
    let after = i + 4;
    if after < source.len() {
        let next = source[after..].chars().next()?;
        if next.is_alphanumeric() || next == '_' {
            return None;
        }
    }
    let mut p = after;
    while p < source.len() {
        let ch = source[p..].chars().next()?;
        if !ch.is_whitespace() {
            break;
        }
        p += ch.len_utf8();
    }
    (source.as_bytes().get(p) == Some(&b'{')).then_some(p)
}

/// 字节偏移 → (行, 列)，两者都从 1 起，列按**字符**算。
///
/// 报错要带位置。以前这两处只有一句「HTML 模板附近存在未闭合的字符串」——
/// 没有行号、没有列号，而且源码里一个 HTML 块都没有时也会这么说，
/// 把人往完全错误的方向引（这个预扫描对每份源码都跑）。
fn 行列(source: &str, offset: usize) -> (usize, usize) {
    let 前 = &source[..offset.min(source.len())];
    let 行 = 前.matches('\n').count() + 1;
    let 列 = 前
        .rsplit('\n')
        .next()
        .map(|l| l.chars().count())
        .unwrap_or(0)
        + 1;
    (行, 列)
}

fn skip_quoted(source: &str, start: usize, quote: char) -> Result<usize, String> {
    let mut i = start + quote.len_utf8();
    while i < source.len() {
        let ch = source[i..].chars().next().unwrap();
        if quote != '`' && ch == '\\' {
            i += ch.len_utf8();
            if i < source.len() {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
            continue;
        }
        i += ch.len_utf8();
        if ch == quote {
            return Ok(i);
        }
    }
    let (行, 列) = 行列(source, start);
    Err(format!(
        "未闭合的字符串字面量（第 {} 行第 {} 列起）",
        行, 列
    ))
}

fn skip_block_comment(source: &str, start: usize) -> Result<usize, String> {
    let mut i = start + 2;
    let mut depth = 1usize;
    while i < source.len() {
        if source[i..].starts_with("/*") {
            depth += 1;
            i += 2;
        } else if source[i..].starts_with("*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Ok(i);
            }
        } else {
            i += source[i..].chars().next().unwrap().len_utf8();
        }
    }
    let (行, 列) = 行列(source, start);
    Err(format!("未闭合的块注释（第 {} 行第 {} 列起）", 行, 列))
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    expressions: Vec<DynamicExpression>,
    loop_count: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, pos: usize) -> Self {
        Self {
            source,
            pos,
            expressions: Vec::new(),
            loop_count: 0,
        }
    }

    fn parse_template(mut self) -> HtmlResult<HtmlTemplate> {
        let children = self.parse_children(None)?;
        self.expect_byte(b'}', "HTML 模板缺少结束 `}`")?;
        let plan = if children.len() == 1 {
            children.into_iter().next().unwrap()
        } else {
            json!({"k":"f","c":children})
        };
        Ok(HtmlTemplate {
            plan,
            expressions: self.expressions,
            end: self.pos,
        })
    }

    fn parse_children(&mut self, closing: Option<&str>) -> HtmlResult<Vec<Value>> {
        let mut nodes = Vec::new();
        let mut text_start = self.pos;
        loop {
            if self.pos >= self.source.len() {
                return Err(self.error("HTML 模板未闭合"));
            }
            if closing.is_none() && self.peek_byte() == Some(b'}') {
                self.push_text(&mut nodes, text_start, self.pos);
                return Ok(nodes);
            }
            if self.source[self.pos..].starts_with("</") {
                self.push_text(&mut nodes, text_start, self.pos);
                let actual = self.parse_closing_tag()?;
                match closing {
                    Some(expected) if expected == actual => return Ok(nodes),
                    Some(expected) => {
                        return Err(self.error(format!(
                            "HTML 标签不匹配：期望 </{expected}>，实际 </{actual}>"
                        )))
                    }
                    None => {
                        return Err(self.error(format!("HTML 模板出现多余的结束标签 </{actual}>")))
                    }
                }
            }
            match self.peek_byte() {
                Some(b'<') => {
                    self.push_text(&mut nodes, text_start, self.pos);
                    nodes.push(self.parse_element()?);
                    text_start = self.pos;
                }
                Some(b'{') => {
                    self.push_text(&mut nodes, text_start, self.pos);
                    let index = self.parse_expression(DynamicContext::Body)?;
                    nodes.push(json!({"k":"d","i":index}));
                    text_start = self.pos;
                }
                _ => self.bump_char(),
            }
        }
    }

    fn parse_element(&mut self) -> HtmlResult<Value> {
        let expression_base = self.expressions.len();
        self.expect_byte(b'<', "HTML 元素缺少 `<`")?;
        if self.source[self.pos..].starts_with('!') || self.source[self.pos..].starts_with('?') {
            return Err(self.error("HTML 模板第一版暂不支持注释、DOCTYPE 或处理指令"));
        }
        let tag = self.parse_name("HTML 标签名")?;
        let lower = tag.to_ascii_lowercase();
        if lower == "script" || lower == "style" {
            return Err(self.error(format!("HTML 模板第一版不支持 <{tag}>，请使用静态资源文件")));
        }
        let mut attrs = Vec::new();
        let mut condition = None;
        let mut loop_binding = None;
        let mut self_closing = false;
        loop {
            self.skip_ws();
            if self.source[self.pos..].starts_with("/>") {
                self.pos += 2;
                self_closing = true;
                break;
            }
            if self.peek_byte() == Some(b'>') {
                self.pos += 1;
                break;
            }
            let name = self.parse_name("HTML 属性名")?;
            if name == "对于" {
                if loop_binding.is_some() {
                    return Err(self.error("HTML 元素只能有一个 `对于` 循环"));
                }
                self.skip_ws();
                self.expect_byte(b'=', "循环属性 `对于` 缺少 `=`")?;
                self.skip_ws();
                if self.peek_byte() != Some(b'{') {
                    return Err(self.error("循环属性 `对于` 必须写成 `对于={项 在 数组}`"));
                }
                loop_binding = Some(self.parse_loop_binding()?);
                continue;
            }
            if name == "如果" {
                if condition.is_some() {
                    return Err(self.error("HTML 元素只能有一个 `如果` 条件"));
                }
                self.skip_ws();
                self.expect_byte(b'=', "条件属性 `如果` 缺少 `=`")?;
                self.skip_ws();
                if self.peek_byte() != Some(b'{') {
                    return Err(self.error("条件属性 `如果` 必须写成 `如果={布尔表达式}`"));
                }
                condition = Some(self.parse_expression(DynamicContext::Condition)?);
                continue;
            }
            if name == "键" {
                self.skip_ws();
                self.expect_byte(b'=', "键属性 `键` 缺少 `=`")?;
                self.skip_ws();
                if self.peek_byte() != Some(b'{') {
                    return Err(self.error("键属性 `键` 必须写成 `键={字符串或整数表达式}`"));
                }
                let index = self.parse_expression(DynamicContext::Key)?;
                attrs.push(json!({"n":"data-键","k":"d","i":index}));
                continue;
            }
            let attr_lower = name.to_ascii_lowercase();
            if attr_lower.starts_with("on") {
                return Err(self.error(format!(
                    "HTML 模板不允许内联事件属性 `{name}`，请使用 data-* 事件"
                )));
            }
            self.skip_ws();
            if self.peek_byte() != Some(b'=') {
                attrs.push(json!({"n":name,"k":"b"}));
                continue;
            }
            self.pos += 1;
            self.skip_ws();
            match self.peek_byte() {
                Some(b'\"') | Some(b'\'') => {
                    let quote = self.source[self.pos..].chars().next().unwrap();
                    let start = self.pos + quote.len_utf8();
                    let end = skip_quoted(self.source, self.pos, quote)
                        .map_err(|message| self.error(message))?;
                    let value_end = end - quote.len_utf8();
                    let value = &self.source[start..value_end];
                    if value.contains('{') || value.contains('}') {
                        return Err(self.error(format!(
                            "属性 `{name}` 暂不支持字符串内插值，请写 `{name}={{表达式}}`"
                        )));
                    }
                    attrs.push(json!({"n":name,"k":"s","v":value}));
                    self.pos = end;
                }
                Some(b'{') => {
                    let index = self.parse_expression(DynamicContext::Attribute)?;
                    attrs.push(json!({"n":name,"k":"d","i":index}));
                }
                _ => {
                    return Err(self.error(format!("属性 `{name}` 的值必须使用引号或 `{{表达式}}`")))
                }
            }
        }
        let children = if self_closing || is_void_element(&lower) {
            Vec::new()
        } else {
            self.parse_children(Some(&tag))?
        };
        let plan = element_plan(tag, attrs, children, condition);
        if let Some((variable, iterable)) = loop_binding {
            return self.lower_loop(plan, expression_base, variable, iterable);
        }
        Ok(plan)
    }

    fn parse_loop_binding(&mut self) -> HtmlResult<(String, String)> {
        let source = self.parse_braced_source("HTML 循环绑定")?;
        let Some(split) = find_top_level_in(&source) else {
            return Err(self.error("HTML 循环必须写成 `对于={项 在 数组}`"));
        };
        let variable = source[..split].trim();
        let iterable = source[split + "在".len()..].trim();
        if !is_qi_identifier(variable) || iterable.is_empty() {
            return Err(self.error("HTML 循环必须写成 `对于={项 在 数组}`"));
        }
        Ok((variable.to_string(), iterable.to_string()))
    }

    fn lower_loop(
        &mut self,
        mut plan: Value,
        expression_base: usize,
        variable: String,
        iterable: String,
    ) -> HtmlResult<Value> {
        let local_expressions = self.expressions.split_off(expression_base);
        rebase_plan_indexes(&mut plan, expression_base)?;
        let plan_json = serde_json::to_string(&plan.to_string())
            .map_err(|e| self.error(format!("HTML 循环模板计划编码失败: {e}")))?;
        let wrapped = local_expressions
            .iter()
            .map(wrap_dynamic_expression)
            .collect::<Vec<_>>()
            .join(", ");
        let result = format!("__qi_html片段_{}", self.loop_count);
        self.loop_count += 1;
        // 容器用 __qi_html循环开始 而不是 创建片段：所有项共用这一份编译期常量计划，
        // 容器一路攒住每项的槽位数组，列表改一项就只发那一项的那个槽位。
        // 用 创建片段 + 块加子块 的老写法会在第一次追加时清掉出身，整段列表塌成一个槽位。
        let source = format!(
            "__qi_html循环块(闭包(): HTML块 {{ 变量 {result}: HTML块 = __qi_html循环开始({plan_json}); 对于 {variable} 在 {iterable} {{ {result} = __qi_html循环加项({result}, __qi_html模板({plan_json}, [{wrapped}])); }} 返回 {result}; }})"
        );
        let index = self.expressions.len();
        self.expressions.push(DynamicExpression {
            source,
            context: DynamicContext::Body,
        });
        Ok(json!({"k":"d","i":index}))
    }

    fn parse_closing_tag(&mut self) -> HtmlResult<String> {
        self.pos += 2;
        let name = self.parse_name("HTML 结束标签名")?;
        self.skip_ws();
        self.expect_byte(b'>', "HTML 结束标签缺少 `>`")?;
        Ok(name)
    }

    fn parse_expression(&mut self, context: DynamicContext) -> HtmlResult<usize> {
        let expr = self.parse_braced_source("HTML 动态洞")?;
        let index = self.expressions.len();
        self.expressions.push(DynamicExpression {
            source: expr,
            context,
        });
        Ok(index)
    }

    fn parse_braced_source(&mut self, what: &str) -> HtmlResult<String> {
        self.expect_byte(b'{', &format!("{what}缺少 `{{`"))?;
        let start = self.pos;
        let mut depth = 1usize;
        while self.pos < self.source.len() {
            let ch = self.source[self.pos..].chars().next().unwrap();
            if ch == '"' || ch == '\'' || ch == '`' {
                self.pos = skip_quoted(self.source, self.pos, ch)
                    .map_err(|message| self.error(message))?;
                continue;
            }
            if self.source[self.pos..].starts_with("/*") {
                self.pos = skip_block_comment(self.source, self.pos)
                    .map_err(|message| self.error(message))?;
                continue;
            }
            if self.source[self.pos..].starts_with("//") {
                self.pos = self.source[self.pos..]
                    .find('\n')
                    .map(|n| self.pos + n)
                    .unwrap_or(self.source.len());
                continue;
            }
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    let expr = self.source[start..self.pos].trim();
                    if expr.is_empty() {
                        return Err(self.error(format!("{what}不能为空")));
                    }
                    self.pos += 1;
                    return Ok(expr.to_string());
                }
            }
            self.pos += ch.len_utf8();
        }
        Err(self.error(format!("{what}缺少结束 `}}`")))
    }

    fn parse_name(&mut self, what: &str) -> HtmlResult<String> {
        let start = self.pos;
        while self.pos < self.source.len() {
            let ch = self.source[self.pos..].chars().next().unwrap();
            if ch.is_alphanumeric() || matches!(ch, ':' | '_' | '-') {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        let name = &self.source[start..self.pos];
        if self.pos == start
            || (!name.chars().next().unwrap().is_ascii_alphabetic()
                && !matches!(name, "如果" | "对于" | "键"))
        {
            return Err(self.error(format!("非法的{what}")));
        }
        Ok(name.to_string())
    }

    fn push_text(&self, nodes: &mut Vec<Value>, start: usize, end: usize) {
        if end > start {
            nodes.push(json!({"k":"t","v":&self.source[start..end]}));
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.source.len() {
            let ch = self.source[self.pos..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            self.pos += ch.len_utf8();
        }
    }

    fn expect_byte(&mut self, byte: u8, message: &str) -> HtmlResult<()> {
        if self.peek_byte() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn error(&self, message: impl Into<String>) -> HtmlError {
        HtmlError {
            message: message.into(),
            offset: self.pos,
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }
    fn bump_char(&mut self) {
        self.pos += self.source[self.pos..].chars().next().unwrap().len_utf8();
    }
}

fn wrap_dynamic_expression(expression: &DynamicExpression) -> String {
    match expression.context {
        DynamicContext::Body => format!("__qi_html正文值({})", expression.source),
        DynamicContext::Attribute => format!("__qi_html属性值({})", expression.source),
        DynamicContext::Condition => format!("__qi_html条件值({})", expression.source),
        DynamicContext::Key => format!("__qi_html键值({})", expression.source),
    }
}

fn rebase_plan_indexes(value: &mut Value, base: usize) -> HtmlResult<()> {
    match value {
        Value::Array(values) => {
            for value in values {
                rebase_plan_indexes(value, base)?;
            }
        }
        Value::Object(map) => {
            for key in ["i", "q"] {
                if let Some(index) = map.get_mut(key) {
                    let raw = index.as_u64().ok_or_else(|| HtmlError {
                        message: "HTML 模板动态索引无效".to_string(),
                        offset: 0,
                    })? as usize;
                    if raw < base {
                        return Err(HtmlError {
                            message: "HTML 循环模板引用了循环外动态值".to_string(),
                            offset: 0,
                        });
                    }
                    *index = json!(raw - base);
                }
            }
            for value in map.values_mut() {
                rebase_plan_indexes(value, base)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_qi_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
        && !value.starts_with("__qi_html")
}

fn find_top_level_in(source: &str) -> Option<usize> {
    let mut i = 0usize;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    while i < source.len() {
        let ch = source[i..].chars().next()?;
        if ch == '"' || ch == '\'' || ch == '`' {
            i = skip_quoted(source, i, ch).ok()?;
            continue;
        }
        if source[i..].starts_with("/*") {
            i = skip_block_comment(source, i).ok()?;
            continue;
        }
        if source[i..].starts_with("//") {
            i = source[i..]
                .find('\n')
                .map(|n| i + n)
                .unwrap_or(source.len());
            continue;
        }
        match ch {
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            '在' if paren == 0 && bracket == 0 && brace == 0 => {
                let before_ok = i == 0
                    || source[..i]
                        .chars()
                        .next_back()
                        .map(|c| c.is_whitespace())
                        .unwrap_or(true);
                let end = i + ch.len_utf8();
                let after_ok = end == source.len()
                    || source[end..]
                        .chars()
                        .next()
                        .map(|c| c.is_whitespace())
                        .unwrap_or(true);
                if before_ok && after_ok {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += ch.len_utf8();
    }
    None
}

fn element_plan(
    tag: String,
    attrs: Vec<Value>,
    children: Vec<Value>,
    condition: Option<usize>,
) -> Value {
    match condition {
        Some(index) => json!({"k":"e","n":tag,"a":attrs,"c":children,"q":index}),
        None => json!({"k":"e","n":tag,"a":attrs,"c":children}),
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_nested_html() {
        let src =
            "变量 x = HTML { <div class=\"card\"><h1>{标题}</h1><a href={链接}>打开</a></div> };";
        let (out, found) = rewrite_html_templates(src).unwrap();
        assert!(found);
        assert!(out.contains("__qi_html模板("));
        assert!(out.contains("__qi_html正文值(标题)"));
        assert!(out.contains("__qi_html属性值(链接)"));
    }

    #[test]
    fn rewrites_element_condition_without_emitting_attribute() {
        let src = "变量 x = HTML { <section 如果={已登录} class=\"member\">内容</section> };";
        let (out, found) = rewrite_html_templates(src).unwrap();
        assert!(found);
        assert!(out.contains("__qi_html条件值(已登录)"));
        assert!(out.contains(r#"\"q\":0"#));
        assert!(!out.contains(r#"\"n\":\"如果\""#));
    }

    #[test]
    fn condition_syntax_error_keeps_multiline_position() {
        let err = rewrite_html_templates(
            "函数 页面() {\n  返回 HTML {\n    <div\n      如果=\"真\">内容</div>\n  };\n}",
        )
        .unwrap_err();
        assert!(err.contains("必须写成 `如果={布尔表达式}`"));
        assert!(err.contains("第 4 行第 10 列"), "{err}");
    }

    #[test]
    fn rejects_duplicate_element_conditions() {
        let err = rewrite_html_templates("变量 x = HTML { <div 如果={真} 如果={假}>内容</div> };")
            .unwrap_err();
        assert!(err.contains("只能有一个 `如果` 条件"));
    }

    #[test]
    fn lowers_html_loop_into_native_for_scope() {
        let src = "变量 x = HTML { <ul><li 对于={项 在 项们} 键={项.编号}>{项.标题}</li></ul> };";
        let (out, found) = rewrite_html_templates(src).unwrap();
        assert!(found);
        assert!(out.contains("__qi_html循环块(闭包(): HTML块"), "{out}");
        assert!(out.contains("对于 项 在 项们"), "{out}");
        assert!(out.contains("__qi_html键值(项.编号)"), "{out}");
        assert!(out.contains("__qi_html正文值(项.标题)"), "{out}");
        assert!(out.contains(r#"\"n\":\"data-键\""#), "{out}");
    }

    #[test]
    fn rejects_invalid_or_duplicate_html_loops() {
        let invalid = rewrite_html_templates("变量 x = HTML { <p 对于={项}>x</p> };").unwrap_err();
        assert!(invalid.contains("项 在 数组"));

        let duplicate =
            rewrite_html_templates("变量 x = HTML { <p 对于={项 在 甲} 对于={项 在 乙}>x</p> };")
                .unwrap_err();
        assert!(duplicate.contains("只能有一个 `对于`"));
    }

    #[test]
    fn rejects_mismatched_tags() {
        let err = rewrite_html_templates("函数 页面() {\n  返回 HTML {\n    <div></span>\n  };\n}")
            .unwrap_err();
        assert!(err.contains("标签不匹配"));
        assert!(err.contains("第 3 行第"));
    }

    #[test]
    fn rejects_code_contexts() {
        let event =
            rewrite_html_templates("变量 x = HTML { <button onclick=\"x()\">点</button> };")
                .unwrap_err();
        assert!(event.contains("内联事件"));
        let script =
            rewrite_html_templates("变量 x = HTML { <script>{代码}</script> };").unwrap_err();
        assert!(script.contains("不支持 <script>"));
    }

    #[test]
    fn keeps_non_template_html_identifier() {
        let src = "变量 HTML值: 字符串 = \"普通标识符\";";
        let (out, found) = rewrite_html_templates(src).unwrap();
        assert!(!found);
        assert_eq!(out, src);
    }

    #[test]
    fn rejects_source_use_of_html_intrinsics_but_ignores_strings_and_comments() {
        let err = rewrite_html_templates("变量 x = __qi_html正文值(\"坏\");").unwrap_err();
        assert!(err.contains("编译器保留"));
        assert!(err.contains("第 1 行第 8 列"), "{err}");

        let src = "变量 a = \"__qi_html模板\"; // __qi_html属性值\n变量 b = 1;";
        let (out, found) = rewrite_html_templates(src).unwrap();
        assert!(!found);
        assert_eq!(out, src);

        let declaration =
            "// 内部运行时\n包 Web.HTML块;\n函数 __qi_html模板(计划: 字符串) : 字符串 { 返回 计划; }";
        let (out, found) = rewrite_html_templates(declaration).unwrap();
        assert!(!found);
        assert_eq!(out, declaration);

        let spoof = "包 主程序; 函数 __qi_html模板() {}";
        assert!(rewrite_html_templates(spoof)
            .unwrap_err()
            .contains("编译器保留"));
    }
}
