use core::fmt::Write as _;

use super::{COUNT, Command, Preset, PresetPath, read, render};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// A definitions file holding `presets`, then fillers named `filler000` on until it holds
/// `count`.
fn file(presets: &str, count: usize) -> Vec<u8> {
    let mut text = String::from("<presetShapeDefinitons>");
    text.push_str(presets);
    let given = presets.matches("<pathLst").count();
    for index in given..count {
        write!(
            text,
            "<filler{index:03}><pathLst xmlns=\"{A}\"><path><close/></path></pathLst></filler{index:03}>"
        )
        .unwrap();
    }
    text.push_str("</presetShapeDefinitons>");
    text.into_bytes()
}

/// The preset `name` of `presets`.
fn named<'list>(presets: &'list [Preset], name: &str) -> &'list Preset {
    presets.iter().find(|preset| preset.name == name).unwrap()
}

#[test]
fn reads_the_guides_the_paths_and_the_rectangle() {
    let shape = format!(
        "<arrow><avLst xmlns=\"{A}\"><gd name=\"adj1\" fmla=\"val 50000\"/></avLst>\
         <gdLst xmlns=\"{A}\"><gd name=\"x1\" fmla=\"*/ w adj1 100000\"/></gdLst>\
         <ahLst xmlns=\"{A}\"/><cxnLst xmlns=\"{A}\"/>\
         <rect l=\"l\" t=\"t\" r=\"x1\" b=\"b\" xmlns=\"{A}\"/>\
         <pathLst xmlns=\"{A}\"><path w=\"2\" h=\"3\" fill=\"darken\" stroke=\"false\">\
         <moveTo><pt x=\"l\" y=\"t\"/></moveTo><lnTo><pt x=\"x1\" y=\"t\"/></lnTo>\
         <arcTo wR=\"wd2\" hR=\"hd2\" stAng=\"0\" swAng=\"cd4\"/>\
         <quadBezTo><pt x=\"1\" y=\"2\"/><pt x=\"3\" y=\"4\"/></quadBezTo>\
         <cubicBezTo><pt x=\"1\" y=\"2\"/><pt x=\"3\" y=\"4\"/><pt x=\"5\" y=\"6\"/></cubicBezTo>\
         <close/></path></pathLst></arrow>"
    );
    let presets = read(&file(&shape, COUNT)).unwrap();
    let pair = |x: &str, y: &str| (x.to_owned(), y.to_owned());
    let arrow = named(&presets, "arrow");
    assert_eq!(arrow.adjust, [pair("adj1", "val 50000")]);
    assert_eq!(arrow.guides, [pair("x1", "*/ w adj1 100000")]);
    assert_eq!(arrow.rect, Some(["l", "t", "x1", "b"].map(str::to_owned)));
    assert_eq!(
        arrow.paths,
        [PresetPath {
            commands: vec![
                Command::MoveTo("l".to_owned(), "t".to_owned()),
                Command::LineTo("x1".to_owned(), "t".to_owned()),
                Command::ArcTo {
                    h_r: "hd2".to_owned(),
                    st_ang: "0".to_owned(),
                    sw_ang: "cd4".to_owned(),
                    w_r: "wd2".to_owned(),
                },
                Command::QuadTo([pair("1", "2"), pair("3", "4")]),
                Command::CubicTo([pair("1", "2"), pair("3", "4"), pair("5", "6")]),
                Command::Close,
            ],
            fill: "darken".to_owned(),
            h: Some("3".to_owned()),
            stroke: false,
            w: Some("2".to_owned()),
        }]
    );
}

#[test]
fn a_path_without_fill_or_stroke_is_filled_and_stroked() {
    let presets = read(&file("", COUNT)).unwrap();
    let path = &presets[0].paths[0];
    assert_eq!(path.fill, "norm");
    assert!(path.stroke);
    assert_eq!((path.w.as_deref(), path.h.as_deref()), (None, None));
    assert_eq!(presets[0].rect, None);
}

#[test]
fn sorts_the_presets_by_name() {
    let shapes = format!(
        "<zigzag><pathLst xmlns=\"{A}\"><path/></pathLst></zigzag>\
         <arc><pathLst xmlns=\"{A}\"><path/></pathLst></arc>"
    );
    let presets = read(&file(&shapes, COUNT)).unwrap();
    assert_eq!(presets[0].name, "arc");
    assert_eq!(presets[COUNT - 1].name, "zigzag");
}

#[test]
fn writes_a_name_defined_twice_alike_once() {
    let twice = format!("<twice><pathLst xmlns=\"{A}\"><path><close/></path></pathLst></twice>");
    let presets = read(&file(&twice.repeat(2), COUNT)).unwrap();
    assert_eq!(presets.len(), COUNT - 1);
    assert_eq!(
        presets
            .iter()
            .filter(|preset| preset.name == "twice")
            .count(),
        1
    );
}

#[test]
fn refuses_a_name_defined_twice_differently() {
    let shapes = format!(
        "<twice><pathLst xmlns=\"{A}\"><path><close/></path></pathLst></twice>\
         <twice><pathLst xmlns=\"{A}\"><path fill=\"none\"><close/></path></pathLst></twice>"
    );
    let error = read(&file(&shapes, COUNT)).unwrap_err();
    assert_eq!(error.to_string(), "twice is defined twice, differently");
}

#[test]
fn refuses_a_formula_that_is_none_of_the_17() {
    let shape = format!(
        "<odd><gdLst xmlns=\"{A}\"><gd name=\"x\" fmla=\"log w\"/></gdLst>\
         <pathLst xmlns=\"{A}\"/></odd>"
    );
    let error = read(&file(&shape, COUNT)).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "in odd: the guide x has the formula `log w`, none of the 17"
    );
}

#[test]
fn keeps_an_argument_past_the_formulas_own() {
    let shape = format!(
        "<odd><gdLst xmlns=\"{A}\"><gd name=\"xB\" fmla=\"+- xH 0 dxB 0\"/></gdLst>\
         <pathLst xmlns=\"{A}\"/></odd>"
    );
    let presets = read(&file(&shape, COUNT)).unwrap();
    assert_eq!(
        named(&presets, "odd").guides,
        [("xB".to_owned(), "+- xH 0 dxB 0".to_owned())]
    );
}

#[test]
fn refuses_a_path_command_it_does_not_know() {
    let shape = format!("<odd><pathLst xmlns=\"{A}\"><path><spiralTo/></path></pathLst></odd>");
    let error = read(&file(&shape, COUNT)).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "in odd: a path command spiralTo of 0 points"
    );
}

#[test]
fn refuses_a_line_of_two_points() {
    let shape = format!(
        "<odd><pathLst xmlns=\"{A}\"><path><lnTo><pt x=\"0\" y=\"0\"/><pt x=\"1\" y=\"1\"/>\
         </lnTo></path></pathLst></odd>"
    );
    let error = read(&file(&shape, COUNT)).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "in odd: a path command lnTo of 2 points"
    );
}

#[test]
fn refuses_a_count_other_than_187() {
    let error = read(&file("", COUNT - 1)).unwrap_err();
    assert_eq!(error.to_string(), "186 presets, not 187");
}

#[test]
fn renders_the_table_as_rustfmt_lays_it_out() {
    let preset = Preset {
        adjust: vec![("adj".to_owned(), "val 50000".to_owned())],
        guides: Vec::new(),
        name: "half".to_owned(),
        paths: vec![PresetPath {
            commands: vec![
                Command::MoveTo("l".to_owned(), "t".to_owned()),
                Command::ArcTo {
                    h_r: "hd2".to_owned(),
                    st_ang: "cd2".to_owned(),
                    sw_ang: "cd4".to_owned(),
                    w_r: "wd2".to_owned(),
                },
                Command::Close,
            ],
            fill: "norm".to_owned(),
            h: None,
            stroke: true,
            w: Some("10".to_owned()),
        }],
        rect: Some(["l", "t", "r", "b"].map(str::to_owned)),
    };
    let expected = "// header

//! The preset shapes of ECMA-376 Part 1's `presetShapeDefinitions.xml`, section 20.1.10.56.

use super::{Command, Preset, PresetPath};

/// The 1 preset shapes the standard's `presetShapeDefinitions.xml` defines, sorted by name.
pub(super) static PRESETS: [Preset<'static>; 1] = [
    Preset {
        adjust: &[(\"adj\", \"val 50000\")],
        guides: &[],
        name: \"half\",
        paths: &[PresetPath {
            commands: &[
                Command::MoveTo(\"l\", \"t\"),
                Command::ArcTo {
                    h_r: \"hd2\",
                    st_ang: \"cd2\",
                    sw_ang: \"cd4\",
                    w_r: \"wd2\",
                },
                Command::Close,
            ],
            fill: \"norm\",
            h: None,
            stroke: true,
            w: Some(\"10\"),
        }],
        rect: Some([\"l\", \"t\", \"r\", \"b\"]),
    },
];
";
    assert_eq!(render("// header\n", &[preset]), expected);
}
