use super::super::render::layout::{self, Expr};
use super::hook_name;

#[test]
fn a_hook_is_its_readers_name_unless_clippy_keeps_its_prefix_for_selfless_methods() {
    assert_eq!(hook_name("paragraph"), "paragraph");
    assert_eq!(hook_name("r#type"), "r#type");
    assert_eq!(hook_name("from_color"), "node_from_color");
    assert_eq!(hook_name("to_point"), "node_to_point");
    assert_eq!(hook_name("into_shape"), "node_into_shape");
    assert_eq!(hook_name("formula"), "formula");
}

#[test]
fn a_struct_literal_field_too_long_for_its_line_breaks_its_call() {
    let value = Expr::call(
        "markup::transform_child",
        vec![
            Expr::atom("&self.table_style_conditional_formatting_settings_exception_xyz"),
            Expr::atom("ctx"),
        ],
    );
    assert_eq!(
        layout::field(
            12,
            "table_style_conditional_formatting_settings_exception_xyz",
            &value
        ),
        "            table_style_conditional_formatting_settings_exception_xyz: markup::transform_child(\n                \
         &self.table_style_conditional_formatting_settings_exception_xyz,\n                ctx,\n            ),\n"
    );
}
