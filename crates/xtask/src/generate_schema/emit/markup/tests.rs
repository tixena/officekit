use super::render::layout::{self, Expr};

fn call(name: &str, arguments: &[&str]) -> Expr {
    Expr::call(
        name,
        arguments.iter().map(|text| Expr::atom(text)).collect(),
    )
}

#[test]
fn a_statement_that_fits_stays_on_one_line() {
    let mut out = String::new();
    layout::statement(
        "    let mut writer = ",
        &call(
            "crate::Writer::new",
            &["scope", "&super::P_SHAPE", "&node.unknown"],
        ),
        &mut out,
    );
    assert_eq!(
        out,
        "    let mut writer = crate::Writer::new(scope, &super::P_SHAPE, &node.unknown);\n"
    );
}

#[test]
fn a_nested_call_overflows_as_rustfmt_writes_it() {
    let member = Expr::call(
        "PropertySetCustomizedValueMember::PercentageValueWithSign",
        vec![Expr::atom("value")],
    );
    let mut out = String::new();
    layout::line(
        "            return ",
        &Expr::call("Ok", vec![member]),
        ";",
        &mut out,
    );
    assert_eq!(
        out,
        "            return Ok(PropertySetCustomizedValueMember::PercentageValueWithSign(\n                \
         value,\n            ));\n"
    );
}

#[test]
fn a_long_method_call_keeps_its_one_argument_on_the_line() {
    let arm = layout::arm(
        12,
        &Expr::atom("7"),
        &call(
            "field.child",
            &["&mut node.c_nv_grp_sp_pr_non_visual_group_drawing_shape_props"],
        ),
        true,
    );
    assert_eq!(
        arm,
        "            7 => field.child(&mut node.c_nv_grp_sp_pr_non_visual_group_drawing_shape_props),\n"
    );
}

#[test]
fn a_lone_long_array_item_moves_to_the_next_line() {
    let mut out = String::new();
    layout::statement(
        "static DEPTH_PERCENT_WITH_SYMBOL_PATTERNS: &[&str] = ",
        &Expr::Slice(vec![Expr::atom(
            "\"0*(([2-9][0-9])|([1-9][0-9][0-9])|(1[0-9][0-9][0-9])|2000)%\"",
        )]),
        &mut out,
    );
    assert_eq!(
        out,
        "static DEPTH_PERCENT_WITH_SYMBOL_PATTERNS: &[&str] =\n    \
         &[\"0*(([2-9][0-9])|([1-9][0-9][0-9])|(1[0-9][0-9][0-9])|2000)%\"];\n"
    );
}

#[test]
fn literals_read_as_clippy_asks() {
    assert_eq!(layout::integer(0xFFFF_FFFF), "0xFFFF_FFFF");
    assert_eq!(layout::integer(-0x8000_0000), "-0x8000_0000");
    assert_eq!(layout::integer(0x7FFF), "0x7FFF");
    assert_eq!(layout::integer(27_273_042), "27_273_042");
    assert_eq!(layout::integer(1000), "1000");
    assert_eq!(layout::real(1000.0), "1000.0_f64");
    assert_eq!(layout::real(250_000.5), "250_000.5_f64");
    assert_eq!(layout::real(-0.5), "-0.5_f64");
}

#[test]
fn doc_words_that_read_as_code_get_backticks() {
    assert_eq!(
        layout::doc(0, "The HLinks of CT_VectorVariant and w:pPr stay words."),
        "/// The `HLinks` of `CT_VectorVariant` and `w:pPr` stay words.\n"
    );
}

#[test]
fn square_brackets_in_doc_words_are_escaped_so_they_do_not_read_as_links() {
    assert_eq!(
        layout::doc(0, "Blank [False] State"),
        "/// Blank \\[False\\] State\n"
    );
}

#[test]
fn a_long_condition_puts_its_brace_on_a_line_of_its_own() {
    let pattern = Expr::pattern(
        "super::document_properties_custom::CustomFilePropertyContent::BinaryStorage",
        "arm",
    );
    assert_eq!(
        layout::if_let(4, &pattern, "*item"),
        "    if let super::document_properties_custom::CustomFilePropertyContent::BinaryStorage(arm) = *item\n    {\n"
    );
}

#[test]
fn a_method_call_as_a_nested_calls_lone_argument_takes_rustfmts_comma() {
    let mut out = String::new();
    layout::line(
        "        return ",
        &Expr::call(
            "Some",
            vec![Expr::call(
                "super::Model3DExtensionContent::ArcSegment",
                vec![Expr::call("arm.transform", vec![Expr::atom("ctx")])],
            )],
        ),
        ";",
        &mut out,
    );
    assert_eq!(
        out,
        "        return Some(super::Model3DExtensionContent::ArcSegment(\n            arm.transform(ctx),\n        ));\n"
    );
}
