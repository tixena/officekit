//! Rust text laid out as rustfmt lays it out, for the shapes the typed tree's
//! modules use: calls, method calls, tuples, arrays, struct literals,
//! statements, function signatures, `impl` headers, match arms, and doc
//! comments.
//!
//! The expressions follow rustfmt's own rules: a list of arguments goes on
//! one line within its width limit, else its last argument overflows onto
//! the following lines, else each argument takes a line, or short simple
//! ones fill lines.

use core::cmp;
use core::fmt::Write as _;
use core::iter;

/// rustfmt's widest array items on one line.
const ARRAY_WIDTH: usize = 60;
/// rustfmt's widest call or tuple arguments on one line.
const CALL_WIDTH: usize = 60;
/// rustfmt's widest chain of more than one call on one line.
const CHAIN_WIDTH: usize = 60;
/// clippy's smallest decimal literal `decimal_literal_representation` looks at.
const HEX_THRESHOLD: u128 = 0x4000;
pub const INDENT: usize = 4;
/// rustfmt's line width.
pub const MAX_WIDTH: usize = 100;
/// rustfmt's widest list item it still fills lines with.
const SHORT_ITEM_WIDTH: usize = 10;
/// rustfmt's widest struct literal fields on one line.
const STRUCT_WIDTH: usize = 18;

/// A Rust expression of a generated file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Expr {
    /// Written as is: a literal, a path, a field, or a short call.
    Atom(String),
    /// `name(arguments)`.
    Call(String, Vec<Self>),
    /// `receiver.method(arguments)`, the receiver a name.
    Method(String, Vec<Self>),
    /// A tuple struct pattern, `Path(items)`.
    Pattern(String, Vec<Self>),
    /// `prefix inner`: `&inner`, or `return inner`.
    Prefix(String, Box<Self>),
    /// `&[items]`.
    Slice(Vec<Self>),
    /// `Path { field: value, ... }`.
    Struct(String, Vec<(String, Self)>),
    /// `(items)`.
    Tuple(Vec<Self>),
}

/// How a list of items is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tactic {
    Horizontal,
    Mixed,
    Vertical,
}

/// Where an expression goes: the block indent of its lines, the columns its
/// first line starts past that indent, and the columns it may take there.
#[derive(Clone, Copy, Debug)]
struct Shape {
    indent: usize,
    offset: usize,
    one_line_chain: bool,
    width: usize,
}

impl Expr {
    /// An atom.
    #[must_use]
    pub fn atom(text: &str) -> Self {
        Self::Atom(text.to_owned())
    }

    /// A call of `name`, a method call when `name` is `receiver.method`.
    #[must_use]
    pub fn call(name: &str, arguments: Vec<Self>) -> Self {
        if name.contains('.') {
            Self::Method(name.to_owned(), arguments)
        } else {
            Self::Call(name.to_owned(), arguments)
        }
    }

    /// Whether rustfmt lets the expression, a list's last item, overflow
    /// onto the lines after the list's opening.
    fn can_overflow(&self, count: usize) -> bool {
        match self {
            Self::Atom(text) => count == 1 && unprefixed(text).contains('('),
            Self::Call(_, _)
            | Self::Method(_, _)
            | Self::Pattern(_, _)
            | Self::Slice(_)
            | Self::Struct(_, _)
            | Self::Tuple(_) => count == 1,
            Self::Prefix(prefix, inner) => prefix == "&" && inner.can_overflow(count),
        }
    }

    /// rustfmt's `is_method_call`: a method call, maybe borrowed, negated or
    /// otherwise prefixed.
    fn is_method_call(&self) -> bool {
        match self {
            Self::Method(_, _) => true,
            Self::Prefix(_, inner) => inner.is_method_call(),
            Self::Atom(_)
            | Self::Call(_, _)
            | Self::Pattern(_, _)
            | Self::Slice(_)
            | Self::Struct(_, _)
            | Self::Tuple(_) => false,
        }
    }

    /// Whether rustfmt counts the expression a call nested in a list.
    fn is_nested_call(&self) -> bool {
        match self {
            Self::Atom(text) => {
                let bare = unprefixed(text);
                bare.split_once('(')
                    .is_some_and(|(callee, _)| !callee.contains('.'))
            }
            Self::Call(_, _) => true,
            Self::Prefix(prefix, inner) => prefix == "&" && inner.is_nested_call(),
            Self::Method(_, _)
            | Self::Pattern(_, _)
            | Self::Slice(_)
            | Self::Struct(_, _)
            | Self::Tuple(_) => false,
        }
    }

    /// Whether rustfmt counts the expression simple: a literal, a name, or a
    /// field of one, maybe borrowed or negated.
    fn is_simple(&self) -> bool {
        match self {
            Self::Atom(text) => {
                let bare = unprefixed(text);
                bare.starts_with('"')
                    || (!bare.contains("::") && !bare.contains('(') && !bare.contains(' '))
            }
            Self::Prefix(prefix, inner) => prefix == "&" && inner.is_simple(),
            Self::Call(_, _)
            | Self::Method(_, _)
            | Self::Pattern(_, _)
            | Self::Slice(_)
            | Self::Struct(_, _)
            | Self::Tuple(_) => false,
        }
    }

    /// The expression on one line, whatever its width.
    #[must_use]
    pub fn one_line(&self) -> String {
        let joined = |items: &[Self]| {
            items
                .iter()
                .map(Self::one_line)
                .collect::<Vec<String>>()
                .join(", ")
        };
        match self {
            Self::Atom(text) => text.clone(),
            Self::Call(name, items) | Self::Method(name, items) | Self::Pattern(name, items) => {
                format!("{name}({})", joined(items))
            }
            Self::Prefix(prefix, inner) => format!("{prefix}{}", inner.one_line()),
            Self::Slice(items) => format!("&[{}]", joined(items)),
            Self::Struct(path, fields) if fields.is_empty() => format!("{path} {{}}"),
            Self::Struct(path, fields) => {
                let body = fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", value.one_line()))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("{path} {{ {body} }}")
            }
            Self::Tuple(items) => format!("({})", joined(items)),
        }
    }

    /// A tuple struct pattern binding one name, `path(binding)`.
    #[must_use]
    pub fn pattern(path: &str, binding: &str) -> Self {
        Self::Pattern(path.to_owned(), vec![Self::atom(binding)])
    }

    /// A `&inner` expression.
    #[must_use]
    pub fn reference(inner: Self) -> Self {
        Self::Prefix("&".to_owned(), Box::new(inner))
    }

    /// The expression laid out in `shape`, as rustfmt lays it out; `None`
    /// when it does not fit.
    fn rewrite(&self, shape: Shape) -> Option<String> {
        match self {
            Self::Atom(text) => {
                (text.starts_with('"') || text.len() <= shape.width).then(|| text.clone())
            }
            Self::Call(name, items) => {
                if name.len() > shape.width {
                    return None;
                }
                overflow(name, items, shape, CALL_WIDTH, ["(", ")"])
            }
            Self::Method(name, items) => method_chain(name, items, shape),
            Self::Pattern(name, items) => {
                if name.len() > shape.width {
                    return None;
                }
                overflow(name, items, shape, MAX_WIDTH, ["(", ")"])
            }
            Self::Prefix(prefix, inner) => {
                let rest = inner.rewrite(shape.offset_left(prefix.len())?)?;
                Some(format!("{prefix}{rest}"))
            }
            Self::Slice(items) => {
                let list = overflow("", items, shape.offset_left(1)?, ARRAY_WIDTH, ["[", "]"])?;
                Some(format!("&{list}"))
            }
            Self::Struct(path, fields) => literal(path, fields, shape),
            Self::Tuple(items) => overflow("", items, shape, CALL_WIDTH, ["(", ")"]),
        }
    }
}

impl Shape {
    /// The whole line from `indent` on.
    const fn indented(indent: usize) -> Self {
        Self {
            indent,
            offset: 0,
            width: MAX_WIDTH.saturating_sub(indent),
            one_line_chain: false,
        }
    }

    /// A line one step in from this shape's block indent.
    const fn nested(self) -> Self {
        Self {
            one_line_chain: self.one_line_chain,
            ..Self::indented(self.indent + INDENT)
        }
    }

    /// The same line, `by` columns further on.
    fn offset_left(self, by: usize) -> Option<Self> {
        Some(Self {
            indent: self.indent,
            offset: self.offset + by,
            width: self.width.checked_sub(by)?,
            one_line_chain: self.one_line_chain,
        })
    }

    /// The columns reserved after the shape on its line.
    const fn overhead(self) -> usize {
        MAX_WIDTH.saturating_sub(self.indent + self.offset + self.width)
    }

    /// The same line, `by` columns narrower.
    fn sub_width(self, by: usize) -> Option<Self> {
        Some(Self {
            width: self.width.checked_sub(by)?,
            ..self
        })
    }
}

/// The text without a leading `&`, `&mut `, or `-`.
fn unprefixed(text: &str) -> &str {
    let unborrowed = text.strip_prefix('&').unwrap_or(text);
    let unmutable = unborrowed.strip_prefix("mut ").unwrap_or(unborrowed);
    unmutable.strip_prefix('-').unwrap_or(unmutable)
}

/// The first line of `text`.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// A delimited list after `ident`, as rustfmt's `overflow` module lays it
/// out; `limit` is the widest the items may be on one line.
fn overflow(
    ident: &str,
    items: &[Expr],
    shape: Shape,
    limit: usize,
    [open, close]: [&str; 2],
) -> Option<String> {
    let one_line_width = shape.width.saturating_sub(ident.len() + 2);
    let one_line_shape = shape
        .offset_left(ident.len() + 1)
        .and_then(|inner| inner.sub_width(1))
        .unwrap_or(Shape { width: 0, ..shape });
    let nested = shape.nested().sub_width(1)?;
    let mut list: Vec<Option<String>> = items.iter().map(|item| item.rewrite(nested)).collect();
    let tactic = items.last().map_or(Tactic::Horizontal, |last| {
        let shapes = [one_line_shape, nested];
        overflow_last(
            ident,
            items,
            last,
            &mut list,
            shapes,
            [one_line_width, limit],
        )
    });
    let body = write_list(&list, tactic, nested)?;
    let extendable =
        body.is_empty() || first_line(&body).len() < shape.width.saturating_sub(ident.len());
    let mut out = format!("{ident}{open}");
    if tactic == Tactic::Horizontal && extendable {
        out.push_str(&body);
    } else {
        if !body.is_empty() {
            out.push('\n');
            pad(nested.indent, &mut out);
            out.push_str(&body);
        }
        out.push('\n');
        pad(shape.indent, &mut out);
    }
    out.push_str(close);
    Some(out)
}

/// rustfmt's `try_overflow_last_item`: the tactic of a list, its last item
/// overflowing when that suits it.
fn overflow_last(
    ident: &str,
    items: &[Expr],
    last: &Expr,
    rendered: &mut [Option<String>],
    [one_line_shape, nested]: [Shape; 2],
    [one_line_width, limit]: [usize; 2],
) -> Tactic {
    let count = items.len();
    let combine = count == 1 && ident.len() < INDENT;
    let overflows = combine || last.can_overflow(count);
    let mut placeholder = None;
    if overflows {
        let arg_shape = if count == 1 && !last.is_nested_call() {
            Some(one_line_shape)
        } else {
            let before: usize = rendered
                .iter()
                .take(count - 1)
                .map(|item| 2 + item.as_ref().map_or(0, String::len))
                .sum();
            Shape {
                width: cmp::min(limit, one_line_shape.width),
                ..one_line_shape
            }
            .offset_left(before)
        };
        let forced = !combine && last.is_method_call();
        let attempt = arg_shape.and_then(|inner| {
            last.rewrite(Shape {
                one_line_chain: inner.one_line_chain || forced,
                ..inner
            })
        });
        if let Some(rewritten) = attempt {
            if let Some(slot) = rendered.last_mut() {
                *slot = Some(first_line(&rewritten).to_owned());
            }
            placeholder = Some(rewritten);
        }
    }
    let first = definitive(rendered, cmp::min(one_line_width, limit));
    if let (true, Tactic::Horizontal, Some(overflowed)) = (overflows, first, placeholder) {
        let chosen = if count == 1 && overflowed.matches('\n').count() == 1 {
            last.rewrite(nested)
                .filter(|text| !text.contains('\n'))
                .unwrap_or(overflowed)
        } else {
            overflowed
        };
        if let Some(slot) = rendered.last_mut() {
            *slot = Some(chosen);
        }
        return Tactic::Horizontal;
    }
    if let Some(slot) = rendered.last_mut() {
        *slot = last.rewrite(nested);
    }
    let lone = rendered.first().and_then(Option::as_ref);
    if count == 1
        && one_line_width != 0
        && lone.is_some_and(|text| !text.contains('\n') && text.len() <= one_line_width)
    {
        return Tactic::Horizontal;
    }
    let fallback = definitive(rendered, cmp::min(one_line_width, limit));
    let short = rendered
        .iter()
        .all(|item| item.as_ref().map_or(0, String::len) <= SHORT_ITEM_WIDTH);
    if fallback == Tactic::Vertical && short && items.iter().all(Expr::is_simple) {
        Tactic::Mixed
    } else {
        fallback
    }
}

/// rustfmt's `definitive_tactic` for a list limited to `limit` columns on one line.
fn definitive(list: &[Option<String>], limit: usize) -> Tactic {
    let total: usize = list
        .iter()
        .map(|item| item.as_ref().map_or(0, String::len))
        .sum::<usize>()
        + 2 * list.len().saturating_sub(1);
    let multiline = list
        .iter()
        .any(|item| item.as_ref().is_some_and(|text| text.contains('\n')));
    if total <= limit && !multiline {
        Tactic::Horizontal
    } else {
        Tactic::Vertical
    }
}

/// rustfmt's `write_list`: the items joined as the tactic lays them out.
fn write_list(list: &[Option<String>], tactic: Tactic, nested: Shape) -> Option<String> {
    let mut out = String::new();
    let mut line = 0;
    for (index, item) in list.iter().enumerate() {
        let text = item.as_ref()?;
        match tactic {
            Tactic::Horizontal => {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(text);
            }
            Tactic::Vertical => {
                if index > 0 {
                    out.push('\n');
                    pad(nested.indent, &mut out);
                }
                out.push_str(text);
                out.push(',');
            }
            Tactic::Mixed => {
                let width = text.len() + 1;
                if line > 0 && line + 1 + width > nested.width {
                    out.push('\n');
                    pad(nested.indent, &mut out);
                    line = 0;
                }
                if line > 0 {
                    out.push(' ');
                    line += 1;
                }
                out.push_str(text);
                out.push(',');
                line += width;
            }
        }
    }
    Some(out)
}

/// A method call on a name, as rustfmt lays out a chain of one call: on the
/// name's line when it fits there, else on the next line one step in when
/// that takes fewer lines. Under `one_line_chain` (rustfmt's
/// `force_one_line_chain`, set by `try_overflow_last_item` for a method call
/// that overflows a list) a chain on two lines is refused, as `join_rewrites`
/// in rustfmt's `chains.rs` does.
fn method_chain(name: &str, items: &[Expr], shape: Shape) -> Option<String> {
    let (root, method) = name.split_once('.')?;
    if root.len() > shape.width {
        return None;
    }
    let child = format!(".{method}");
    if root.len() <= INDENT.saturating_sub(shape.offset) {
        let rest = overflow(
            &child,
            items,
            shape.offset_left(root.len())?,
            CALL_WIDTH,
            ["(", ")"],
        )?;
        return fits(format!("{root}{rest}"), shape);
    }
    let child_shape = shape.nested();
    let budget = shape.width.saturating_sub(root.len());
    let next_shape = child_shape.sub_width(shape.overhead())?;
    let mut same_line = None;
    if budget > 0
        && let Some(rewritten) = shape
            .offset_left(root.len())
            .and_then(|inner| overflow(&child, items, inner, CALL_WIDTH, ["(", ")"]))
    {
        let lines = rewritten.lines().count();
        let could_fit = first_line(&rewritten).len() <= budget;
        if could_fit && lines >= 5 {
            same_line = Some((rewritten, true));
        } else {
            same_line = Some(
                match overflow(&child, items, next_shape, CALL_WIDTH, ["(", ")"]) {
                    Some(next) if !could_fit => (next, false),
                    Some(next) if next.lines().count() >= lines => (rewritten, could_fit),
                    Some(next) => (next, false),
                    None => (rewritten, could_fit),
                },
            );
        }
    }
    let (text, joined) = if let Some(found) = same_line {
        found
    } else {
        let alone = if budget > 0 { shape } else { next_shape };
        (
            overflow(&child, items, alone, CALL_WIDTH, ["(", ")"])?,
            false,
        )
    };
    if !joined && shape.one_line_chain {
        return None;
    }
    let mut out = String::from(root);
    if !joined {
        out.push('\n');
        pad(child_shape.indent, &mut out);
    }
    out.push_str(&text);
    fits(out, shape)
}

/// The text when it fits `shape` as rustfmt's `wrap_str` checks it.
fn fits(text: String, shape: Shape) -> Option<String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    if first.len() > shape.width {
        return None;
    }
    if !text.contains('\n') {
        return Some(text);
    }
    let rest: Vec<&str> = lines.collect();
    if rest.iter().any(|line| line.len() > MAX_WIDTH) {
        return None;
    }
    let last = rest.last().map_or(0, |line| line.len());
    (last <= shape.indent + shape.offset + shape.width).then_some(text)
}

/// A struct literal, as rustfmt lays it out: its fields on one line within
/// `STRUCT_WIDTH`, else each on its own line.
fn literal(path: &str, fields: &[(String, Expr)], shape: Shape) -> Option<String> {
    if path.len() + 2 > shape.width {
        return None;
    }
    if fields.is_empty() {
        return Some(format!("{path} {{}}"));
    }
    let nested = shape.nested();
    let mut list = Vec::new();
    for (name, value) in fields {
        list.push(field_value(nested, name, value)?);
    }
    let one_line = shape
        .width
        .checked_sub(path.len() + 5)
        .map(|width| cmp::min(width, STRUCT_WIDTH));
    let joined = list.join(", ");
    if let Some(width) = one_line
        && joined.len() <= width
        && !joined.contains('\n')
    {
        return Some(format!("{path} {{ {joined} }}"));
    }
    let mut out = format!("{path} {{");
    for text in &list {
        out.push('\n');
        pad(nested.indent, &mut out);
        out.push_str(text);
        out.push(',');
    }
    out.push('\n');
    pad(shape.indent, &mut out);
    out.push('}');
    Some(out)
}

/// A struct literal's field, `name: value`, on the line `nested` gives it.
fn field_value(nested: Shape, name: &str, value: &Expr) -> Option<String> {
    let after = nested
        .sub_width(1)
        .and_then(|inner| inner.offset_left(name.len() + 2));
    if let Some(rewritten) = after.and_then(|inner| value.rewrite(inner)) {
        return Some(format!("{name}: {rewritten}"));
    }
    let below = nested.nested();
    let rewritten = value.rewrite(below)?;
    let mut out = format!("{name}:\n");
    pad(below.indent, &mut out);
    out.push_str(&rewritten);
    Some(out)
}

/// A method chain ending a block at `indent`: `root` and its `calls`, on one
/// line when it fits there, within `CHAIN_WIDTH` when it has more than one
/// call; else each call on its own line one step in.
#[must_use]
pub fn chain(indent: usize, root: &str, calls: &[String]) -> String {
    let joined = format!("{root}{}", calls.concat());
    let limit = if calls.len() > 1 {
        CHAIN_WIDTH
    } else {
        MAX_WIDTH
    };
    let mut out = String::new();
    pad(indent, &mut out);
    if indent + joined.len() <= MAX_WIDTH && joined.len() <= limit {
        out.push_str(&joined);
        out.push('\n');
        return out;
    }
    out.push_str(root);
    out.push('\n');
    for call in calls {
        pad(indent + INDENT, &mut out);
        out.push_str(call);
        out.push('\n');
    }
    out
}

/// `width` spaces.
pub fn pad(width: usize, out: &mut String) {
    out.extend(iter::repeat_n(' ', width));
}

/// `start`, ending in `= `, then the expression and `;`, laid out as `line`
/// lays it out.
pub fn statement(start: &str, expr: &Expr, out: &mut String) {
    line(start, expr, ";", out);
}

/// `start`, then the expression and `end`. A `start` ending in `= ` begins
/// an assignment, whose expression rustfmt moves to the next line when that
/// suits it; another `start` is a prefix of the expression (`return `), and
/// spaces alone begin an expression statement.
pub fn line(start: &str, expr: &Expr, end: &str, out: &mut String) {
    let indent = start.len() - start.trim_start().len();
    let head = start.trim_start();
    pad(indent, out);
    let shape = Shape::indented(indent).sub_width(end.len());
    let text = head.strip_suffix("= ").map_or_else(
        || {
            if head.is_empty() {
                expr.rewrite(shape?)
            } else {
                // rustfmt keeps one more column free after a `return` expression.
                let prefixed = Expr::Prefix(head.to_owned(), Box::new(expr.clone()));
                prefixed.rewrite(shape?.sub_width(1)?)
            }
        },
        |rest| assignment(&format!("{rest}="), expr, shape),
    );
    out.push_str(&text.unwrap_or_else(|| format!("{head}{}", expr.one_line())));
    out.push_str(end);
    out.push('\n');
}

/// Whether rustfmt lays the expression out on one line at `indent`, with
/// `after` columns following it.
#[must_use]
pub fn fits_one_line(indent: usize, expr: &Expr, after: usize) -> bool {
    Shape::indented(indent)
        .sub_width(after)
        .and_then(|shape| expr.rewrite(shape))
        .is_some_and(|text| !text.contains('\n'))
}

/// `if let pattern = expr {` at `indent`, as rustfmt lays out the condition
/// and the brace after it: the brace on a line of its own when the condition
/// takes more than one line or leaves no room for it.
#[must_use]
pub fn if_let(indent: usize, pattern: &Expr, expr: &str) -> String {
    let shape = Shape::indented(indent);
    let rewritten = shape.offset_left("if ".len()).and_then(|cond_shape| {
        let pattern_text = cond_shape
            .offset_left("let ".len())
            .and_then(|inner| inner.sub_width(" =".len()))
            .and_then(|inner| pattern.rewrite(inner))?;
        assignment(
            &format!("let {pattern_text} ="),
            &Expr::atom(expr),
            Some(cond_shape),
        )
    });
    let condition = rewritten.unwrap_or_else(|| format!("let {} = {expr}", pattern.one_line()));
    let budget = MAX_WIDTH.saturating_sub(indent + "if ".len() + " {".len());
    let mut out = String::new();
    pad(indent, &mut out);
    out.push_str("if ");
    out.push_str(&condition);
    if condition.contains('\n') || condition.len() > budget {
        out.push('\n');
        pad(indent, &mut out);
        out.push_str("{\n");
    } else {
        out.push_str(" {\n");
    }
    out
}

/// rustfmt's `rewrite_assign_rhs`: `lhs =` and the expression after it, on
/// the same line or on the next.
fn assignment(lhs: &str, expr: &Expr, outer: Option<Shape>) -> Option<String> {
    let shape = outer?;
    let last_width = lhs.rsplit_once('\n').map_or(lhs.len(), |(_, last)| {
        last.len().saturating_sub(shape.indent)
    });
    let orig_shape = shape.offset_left(last_width + 1).unwrap_or(Shape {
        width: 0,
        offset: shape.offset + last_width + 1,
        ..shape
    });
    let orig = expr.rewrite(orig_shape);
    if let Some(text) = &orig
        && !text.contains('\n')
        && text.len() <= orig_shape.width
    {
        return Some(format!("{lhs} {text}"));
    }
    let next_shape = orig_shape.nested().sub_width(orig_shape.overhead())?;
    let next = expr.rewrite(next_shape);
    let mut below = format!("{lhs}\n");
    pad(next_shape.indent, &mut below);
    match (orig, next) {
        (Some(same_line), Some(next_line)) if fits(next_line.clone(), next_shape).is_none() => {
            Some(format!("{lhs} {same_line}"))
        }
        (Some(same_line), Some(next_line)) if prefer_next_line(&same_line, &next_line) => {
            below.push_str(&next_line);
            Some(below)
        }
        (None, Some(next_line)) => {
            below.push_str(&next_line);
            Some(below)
        }
        (None, None) => None,
        (Some(same_line), _) => Some(format!("{lhs} {same_line}")),
    }
}

/// rustfmt's `prefer_next_line` for an expression laid out on its line and
/// on the next.
fn prefer_next_line(same: &str, next: &str) -> bool {
    let opens = |text: &str, bracket: char| first_line(text).ends_with(bracket);
    !next.contains('\n')
        || same.matches('\n').count() > next.matches('\n').count() + 1
        || ['(', '{', '[']
            .iter()
            .any(|bracket| opens(same, *bracket) && !opens(next, *bracket))
}

/// A string literal, every character outside printable ASCII escaped.
#[must_use]
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(character),
            _ => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(character));
            }
        }
    }
    out.push('"');
    out
}

/// Decimal digits grouped by three from the right when there are more than five.
#[must_use]
pub fn grouped(digits: &str) -> String {
    if digits.len() <= 5 {
        return digits.to_owned();
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('_');
        }
        out.push(digit);
    }
    out
}

/// A whole number as a literal, its digits grouped as `grouped` does.
#[must_use]
pub fn integer(value: i128) -> String {
    if value == i128::from(i64::MIN) {
        return "i64::MIN".to_owned();
    }
    if value == i128::from(i64::MAX) {
        return "i64::MAX".to_owned();
    }
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    let hex = format!("{magnitude:X}");
    if magnitude >= HEX_THRESHOLD && reads_better_in_hex(&hex) {
        let mut out = format!("{sign}0x");
        for (index, digit) in hex.chars().enumerate() {
            if index > 0 && (hex.len() - index).is_multiple_of(4) {
                out.push('_');
            }
            out.push(digit);
        }
        return out;
    }
    format!("{sign}{}", grouped(&magnitude.to_string()))
}

/// clippy's `decimal_literal_representation` for a literal of four hexadecimal
/// digits or more: a power of two or one less, within a margin, or digits all
/// `0` and `F` after a leading `7` or `F`.
fn reads_better_in_hex(digits: &str) -> bool {
    let Some(first) = digits.chars().next() else {
        return false;
    };
    let rest = &digits[1..];
    let middle = &rest[..rest.len().saturating_sub(1)];
    (matches!(first, '1' | '2' | '4' | '8') && middle.chars().all(|digit| digit == '0'))
        || (matches!(first, '1' | '3' | '7' | 'F') && middle.chars().all(|digit| digit == 'F'))
        || (matches!(first, '7' | 'F') && rest.chars().all(|digit| matches!(digit, '0' | 'F')))
}

/// A number as an `f64` literal: Rust's shortest form of it, the whole part
/// grouped as `grouped` does, with the `f64` suffix.
#[must_use]
pub fn real(value: f64) -> String {
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "f64::INFINITY".to_owned()
        } else {
            "f64::NEG_INFINITY".to_owned()
        };
    }
    let text = format!("{value:?}");
    if text.contains('e') {
        return format!("{text}_f64");
    }
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |rest| ("-", rest));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    format!("{sign}{}.{fraction}_f64", grouped(whole))
}

/// A function's signature: `head(parameters) -> output {` on one line when it
/// fits, else each parameter on a line of its own.
#[must_use]
pub fn signature(indent: usize, head: &str, parameters: &[String], output: &str) -> String {
    let arrow = if output.is_empty() {
        String::new()
    } else {
        format!(" -> {output}")
    };
    let flat = format!("{head}({}){arrow} {{", parameters.join(", "));
    let mut out = String::new();
    if indent + flat.len() <= MAX_WIDTH {
        pad(indent, &mut out);
        out.push_str(&flat);
        out.push('\n');
        return out;
    }
    pad(indent, &mut out);
    out.push_str(head);
    out.push_str("(\n");
    for parameter in parameters {
        pad(indent + INDENT, &mut out);
        out.push_str(parameter);
        out.push_str(",\n");
    }
    pad(indent, &mut out);
    out.push(')');
    let tail = format!("){arrow} {{");
    if indent + tail.len() <= MAX_WIDTH {
        out.push_str(&arrow);
        out.push_str(" {\n");
    } else if indent + tail.len() - " {".len() <= MAX_WIDTH {
        out.push_str(&arrow);
        out.push('\n');
        pad(indent, &mut out);
        out.push_str("{\n");
    } else {
        out.push_str(" -> ");
        out.push_str(&broken_type(indent, output));
        out.push_str(" {\n");
    }
    out
}

/// A generic type too long for its line: each of its arguments on a line of
/// its own one step in, as rustfmt breaks `Result<A, B>`.
fn broken_type(indent: usize, shown: &str) -> String {
    let Some((name, inner)) = shown
        .split_once('<')
        .and_then(|(name, rest)| Some((name, rest.strip_suffix('>')?)))
    else {
        return shown.to_owned();
    };
    let mut arguments = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (index, character) in inner.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                arguments.push(inner[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    arguments.push(inner[start..].trim());
    let mut out = format!("{name}<\n");
    for argument in arguments {
        pad(indent + INDENT, &mut out);
        out.push_str(argument);
        out.push_str(",\n");
    }
    pad(indent, &mut out);
    out.push('>');
    out
}

/// An `impl` header: `impl trait for target {` on one line when it fits, else
/// `for target` on a line of its own and `{` after it.
#[must_use]
pub fn impl_header(trait_part: &str, target: &str) -> String {
    let flat = format!("impl {trait_part} for {target} {{\n");
    if flat.len() - 1 <= MAX_WIDTH {
        return flat;
    }
    format!("impl {trait_part}\n    for {target}\n{{\n")
}

/// A match arm at `indent`: `pattern => body,` on one line when it fits. A
/// `statement` body, a call returning nothing, that does not fit goes in a
/// block ending in `;`; another body goes where rustfmt's
/// `rewrite_match_body` puts it.
#[must_use]
pub fn arm(indent: usize, pattern: &Expr, body: &Expr, statement: bool) -> String {
    let shape = Shape::indented(indent);
    let pattern_text = shape
        .sub_width(5)
        .and_then(|inner| pattern.rewrite(inner))
        .unwrap_or_else(|| pattern.one_line());
    let last_width = pattern_text
        .rsplit_once('\n')
        .map_or(pattern_text.len(), |(_, last)| {
            last.len().saturating_sub(indent)
        });
    let mut out = String::new();
    pad(indent, &mut out);
    out.push_str(&pattern_text);
    let same_shape = shape
        .offset_left(last_width + 4)
        .and_then(|inner| inner.sub_width(1));
    let same = same_shape.and_then(|inner| body.rewrite(inner));
    if let (Some(text), Some(inner)) = (&same, same_shape)
        && !text.contains('\n')
        && text.len() <= inner.width
    {
        let _ = writeln!(out, " => {text},");
        return out;
    }
    let next_shape = shape.nested();
    if statement {
        out.push_str(" => {\n");
        let mut start = String::new();
        pad(next_shape.indent, &mut start);
        line(&start, body, ";", &mut out);
        pad(indent, &mut out);
        out.push_str("}\n");
        return out;
    }
    let next = body.rewrite(next_shape);
    let budget = same_shape.map_or(0, |inner| inner.width);
    let below = match (&same, &next) {
        (Some(same_line), Some(next_line)) if prefer_next_line(same_line, next_line) => true,
        (Some(same_line), _) if first_line(same_line).len() <= budget => false,
        (Some(same_line), Some(_)) if same_line.contains('\n') => true,
        (None, Some(_)) => true,
        (Some(_) | None, _) => false,
    };
    match (below, same, next) {
        (true, _, Some(next_line)) => {
            out.push_str(" => {\n");
            pad(next_shape.indent, &mut out);
            out.push_str(&next_line);
            out.push('\n');
            pad(indent, &mut out);
            out.push_str("}\n");
        }
        (_, Some(same_line), _) => {
            let _ = writeln!(out, " => {same_line},");
        }
        (_, None, _) => {
            let _ = writeln!(out, " => {},", body.one_line());
        }
    }
    out
}

/// Doc comment lines: each paragraph's words filled to the line width, words
/// clippy's `doc_markdown` would take for code put between backticks.
#[must_use]
pub fn doc(indent: usize, text: &str) -> String {
    let mut out = String::new();
    for (index, paragraph) in text.split("\n\n").enumerate() {
        if index > 0 {
            pad(indent, &mut out);
            out.push_str("///\n");
        }
        let width = MAX_WIDTH - indent - 4;
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let shown = code_word(word);
            if !line.is_empty() && line.len() + 1 + shown.len() > width {
                pad(indent, &mut out);
                out.push_str("/// ");
                out.push_str(&line);
                out.push('\n');
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&shown);
        }
        if !line.is_empty() {
            pad(indent, &mut out);
            out.push_str("/// ");
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// A word as `doc_markdown` accepts it: put between backticks when it holds
/// an underscore or a lower-case letter followed by an upper-case one, or is
/// upper camel case as that lint reads it (`HLinks`, `OLEObject`).
fn code_word(word: &str) -> String {
    if word.contains('`') {
        return word.to_owned();
    }
    let core_start = word
        .find(|c: char| c.is_ascii_alphanumeric() || c == '_')
        .unwrap_or(0);
    let core_end = word
        .rfind(|c: char| c.is_ascii_alphanumeric() || c == '_')
        .map_or(word.len(), |end| end + 1);
    if core_start >= core_end {
        return word.to_owned();
    }
    let core = &word[core_start..core_end];
    let characters: Vec<char> = core.chars().collect();
    let camel = characters.windows(2).any(|pair| {
        matches!(pair, [left, right] if left.is_ascii_lowercase() && right.is_ascii_uppercase())
    });
    let digit_camel = characters.windows(2).any(|pair| {
        matches!(pair, [left, right] if left.is_ascii_digit() && right.is_ascii_alphabetic())
    })
        && characters.iter().any(char::is_ascii_uppercase)
        && characters.iter().any(char::is_ascii_lowercase);
    let singular = core.strip_suffix('s').unwrap_or(core);
    let upper_camel = core.starts_with(|c: char| c.is_ascii_uppercase())
        && singular.chars().all(|c| c.is_ascii_alphanumeric())
        && singular.chars().filter(char::is_ascii_uppercase).count() > 1
        && singular.chars().any(|c| c.is_ascii_lowercase());
    if core.contains('_') || camel || digit_camel || upper_camel {
        format!("{}`{core}`{}", &word[..core_start], &word[core_end..])
    } else {
        word.replace('[', "\\[").replace(']', "\\]")
    }
}
