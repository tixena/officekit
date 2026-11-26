//! The preset shapes: ECMA-376 Part 1's `presetShapeDefinitions.xml`, written as the table
//! `crates/officekit/src/drawing/geometry/presets.rs`.

use core::slice;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::element::Element;
use officekit_xml::name::Namespace;
use officekit_xml::parse::parse;
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::markup::render::layout::{self, Expr};
use super::namespaces;
use crate::generate_schema::inputs::Inputs;

#[cfg(test)]
mod tests;

/// The number of definitions the file holds; `upDownArrow` has two, alike.
pub const COUNT: usize = 187;

/// The 17 formulas of section 20.1.9.11.
const FORMULAS: [&str; 17] = [
    "*/", "+-", "+/", "?:", "abs", "at2", "cat2", "cos", "max", "min", "mod", "pin", "sat2", "sin",
    "sqrt", "tan", "val",
];

/// Where the table is written, from the workspace root.
pub const PRESETS_PATH: &str = "crates/officekit/src/drawing/geometry/presets.rs";

/// One step of a path, each value as the file writes it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// `a:arcTo`.
    ArcTo {
        h_r: String,
        st_ang: String,
        sw_ang: String,
        w_r: String,
    },
    /// `a:close`.
    Close,
    /// `a:cubicBezTo`: two control points and the end.
    CubicTo([(String, String); 3]),
    /// `a:lnTo`: x and y.
    LineTo(String, String),
    /// `a:moveTo`: x and y.
    MoveTo(String, String),
    /// `a:quadBezTo`: the control point and the end.
    QuadTo([(String, String); 2]),
}

/// One preset: the children of its `a:custGeom` the outline needs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Preset {
    /// The `a:avLst`: each guide's name and formula.
    pub adjust: Vec<(String, String)>,
    /// The `a:gdLst`: each guide's name and formula.
    pub guides: Vec<(String, String)>,
    pub name: String,
    pub paths: Vec<PresetPath>,
    /// The `a:rect`: `l`, `t`, `r` and `b`.
    pub rect: Option<[String; 4]>,
}

/// One `a:path`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PresetPath {
    pub commands: Vec<Command>,
    /// `fill`, `norm` when absent.
    pub fill: String,
    pub h: Option<String>,
    /// `stroke`, true when absent.
    pub stroke: bool,
    pub w: Option<String>,
}

/// Read the presets, write the table to `out`, and give the number written.
///
/// # Errors
///
/// An input that cannot be read, a file [`read`] refuses, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs, out: &Path) -> Result<usize>
where
    S: System + ?Sized,
{
    let (bytes, _) = ready_now(system.read(&inputs.preset_shapes))
        .with_context(|| format!("cannot read {}", inputs.preset_shapes.display()))?;
    let presets = read(&bytes).with_context(|| format!("in {}", inputs.preset_shapes.display()))?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        slice::from_ref(&inputs.preset_shapes),
        &[],
    )?;
    if let Some(parent) = out.parent() {
        ready_now(system.create_dir_all(parent))
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    ready_now(system.write(out, render(&header, &presets).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(presets.len())
}

/// The presets of `presetShapeDefinitions.xml`, sorted by name, a name defined twice alike
/// once.
///
/// # Errors
///
/// Bytes that are not XML, a formula that is none of the 17, a child or a path command the
/// generator does not know, a count of definitions other than [`COUNT`], or a name defined twice
/// differently.
pub fn read(bytes: &[u8]) -> Result<Vec<Preset>> {
    let document = parse(bytes).context("not XML")?;
    let mut presets = document
        .root
        .child_elements()
        .map(preset)
        .collect::<Result<Vec<Preset>>>()?;
    if presets.len() != COUNT {
        bail!("{} presets, not {COUNT}", presets.len());
    }
    presets.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in presets.windows(2) {
        if let [first, second] = pair
            && first.name == second.name
            && first != second
        {
            bail!("{} is defined twice, differently", first.name);
        }
    }
    presets.dedup();
    Ok(presets)
}

/// The table's text: `header`, then the presets.
#[must_use = "this only returns the value"]
pub fn render(header: &str, presets: &[Preset]) -> String {
    let mut out = format!(
        "{header}\n//! The preset shapes of ECMA-376 Part 1's `presetShapeDefinitions.xml`, \
         section 20.1.10.56.\n\nuse super::{{Command, Preset, PresetPath}};\n\n\
         /// The {count} preset shapes the standard's `presetShapeDefinitions.xml` defines, sorted by \
         name.\n\
         pub(super) static PRESETS: [Preset<'static>; {count}] = [\n",
        count = presets.len(),
    );
    for found in presets {
        layout::line("    ", &preset_expr(found), ",", &mut out);
    }
    out.push_str("];\n");
    out
}

/// The value of the attribute `name`, which the element must have.
fn attribute(element: &Element, name: &str) -> Result<String> {
    element
        .attribute(&Namespace::None, name)
        .map(str::to_owned)
        .with_context(|| format!("an {} with no {name}", element.name.local))
}

/// One step of a path.
fn command(element: &Element) -> Result<Command> {
    let points = element
        .child_elements()
        .map(point)
        .collect::<Result<Vec<(String, String)>>>()?;
    Ok(match (&*element.name.local, points.as_slice()) {
        ("arcTo", []) => Command::ArcTo {
            h_r: attribute(element, "hR")?,
            st_ang: attribute(element, "stAng")?,
            sw_ang: attribute(element, "swAng")?,
            w_r: attribute(element, "wR")?,
        },
        ("close", []) => Command::Close,
        ("cubicBezTo", [first, second, end]) => {
            Command::CubicTo([first.clone(), second.clone(), end.clone()])
        }
        ("lnTo", [(x, y)]) => Command::LineTo(x.clone(), y.clone()),
        ("moveTo", [(x, y)]) => Command::MoveTo(x.clone(), y.clone()),
        ("quadBezTo", [control, end]) => Command::QuadTo([control.clone(), end.clone()]),
        (other, _) => bail!("a path command {other} of {} points", points.len()),
    })
}

/// The expression of one step of a path.
fn command_expr(command: &Command) -> Expr {
    let text = |value: &String| Expr::atom(&layout::string(value));
    let pair = |(x, y): &(String, String)| Expr::Tuple(vec![text(x), text(y)]);
    match command {
        Command::ArcTo {
            h_r,
            st_ang,
            sw_ang,
            w_r,
        } => Expr::Struct(
            "Command::ArcTo".to_owned(),
            vec![
                ("h_r".to_owned(), text(h_r)),
                ("st_ang".to_owned(), text(st_ang)),
                ("sw_ang".to_owned(), text(sw_ang)),
                ("w_r".to_owned(), text(w_r)),
            ],
        ),
        Command::Close => Expr::atom("Command::Close"),
        Command::CubicTo(points) => {
            Expr::call("Command::CubicTo", points.iter().map(pair).collect())
        }
        Command::LineTo(x, y) => Expr::call("Command::LineTo", vec![text(x), text(y)]),
        Command::MoveTo(x, y) => Expr::call("Command::MoveTo", vec![text(x), text(y)]),
        Command::QuadTo(points) => Expr::call("Command::QuadTo", points.iter().map(pair).collect()),
    }
}

/// The name and formula of each `a:gd` of a list, each formula one of the 17.
fn guides(list: &Element) -> Result<Vec<(String, String)>> {
    list.child_elements()
        .map(|guide| {
            if &*guide.name.local != "gd" {
                bail!("an {} in an {}", guide.name.local, list.name.local);
            }
            let name = attribute(guide, "name")?;
            let formula = attribute(guide, "fmla")?;
            let operator = formula.split_whitespace().next().unwrap_or_default();
            if !FORMULAS.contains(&operator) {
                bail!("the guide {name} has the formula `{formula}`, none of the 17");
            }
            Ok((name, formula))
        })
        .collect()
}

/// The expression of an optional text.
fn optional_expr(value: Option<&String>) -> Expr {
    value.map_or_else(
        || Expr::atom("None"),
        |text| Expr::call("Some", vec![Expr::atom(&layout::string(text))]),
    )
}

/// One `a:path`.
fn path(element: &Element) -> Result<PresetPath> {
    if &*element.name.local != "path" {
        bail!("an {} in a pathLst", element.name.local);
    }
    let stroke = match element.attribute(&Namespace::None, "stroke") {
        None | Some("true" | "1") => true,
        Some("false" | "0") => false,
        Some(other) => bail!("a path whose stroke is {other}"),
    };
    let optional = |name: &str| element.attribute(&Namespace::None, name).map(str::to_owned);
    Ok(PresetPath {
        commands: element
            .child_elements()
            .map(command)
            .collect::<Result<Vec<Command>>>()?,
        fill: optional("fill").unwrap_or_else(|| "norm".to_owned()),
        h: optional("h"),
        stroke,
        w: optional("w"),
    })
}

/// The expression of one `a:path`.
fn path_expr(path: &PresetPath) -> Expr {
    Expr::Struct(
        "PresetPath".to_owned(),
        vec![
            (
                "commands".to_owned(),
                Expr::Slice(path.commands.iter().map(command_expr).collect()),
            ),
            ("fill".to_owned(), Expr::atom(&layout::string(&path.fill))),
            ("h".to_owned(), optional_expr(path.h.as_ref())),
            ("stroke".to_owned(), Expr::atom(&path.stroke.to_string())),
            ("w".to_owned(), optional_expr(path.w.as_ref())),
        ],
    )
}

/// The `x` and `y` of an `a:pt`.
fn point(element: &Element) -> Result<(String, String)> {
    if &*element.name.local != "pt" {
        bail!("an {} in a path command", element.name.local);
    }
    Ok((attribute(element, "x")?, attribute(element, "y")?))
}

/// One child of the root: a preset, named by its element.
fn preset(element: &Element) -> Result<Preset> {
    let name = String::from(&*element.name.local);
    let mut found = Preset {
        name: name.clone(),
        ..Preset::default()
    };
    for child in element.child_elements() {
        match &*child.name.local {
            "ahLst" | "cxnLst" => {}
            "avLst" => found.adjust = guides(child).with_context(|| format!("in {name}"))?,
            "gdLst" => found.guides = guides(child).with_context(|| format!("in {name}"))?,
            "pathLst" => {
                found.paths = child
                    .child_elements()
                    .map(path)
                    .collect::<Result<Vec<PresetPath>>>()
                    .with_context(|| format!("in {name}"))?;
            }
            "rect" => {
                found.rect = Some([
                    attribute(child, "l")?,
                    attribute(child, "t")?,
                    attribute(child, "r")?,
                    attribute(child, "b")?,
                ]);
            }
            other => bail!("{name} has a child {other}"),
        }
    }
    Ok(found)
}

/// The expression of one preset.
fn preset_expr(preset: &Preset) -> Expr {
    let text = |value: &String| Expr::atom(&layout::string(value));
    let pairs = |list: &[(String, String)]| {
        Expr::Slice(
            list.iter()
                .map(|(name, formula)| Expr::Tuple(vec![text(name), text(formula)]))
                .collect(),
        )
    };
    let rect = preset.rect.as_ref().map_or_else(
        || Expr::atom("None"),
        |edges| {
            let shown: Vec<String> = edges.iter().map(|edge| layout::string(edge)).collect();
            Expr::call("Some", vec![Expr::atom(&format!("[{}]", shown.join(", ")))])
        },
    );
    Expr::Struct(
        "Preset".to_owned(),
        vec![
            ("adjust".to_owned(), pairs(&preset.adjust)),
            ("guides".to_owned(), pairs(&preset.guides)),
            ("name".to_owned(), text(&preset.name)),
            (
                "paths".to_owned(),
                Expr::Slice(preset.paths.iter().map(path_expr).collect()),
            ),
            ("rect".to_owned(), rect),
        ],
    )
}
