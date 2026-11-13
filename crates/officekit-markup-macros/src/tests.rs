use proc_macro2::TokenStream;
use quote::quote;

use super::{expand, kinds};

fn messages(arguments: TokenStream, item: TokenStream) -> Vec<String> {
    expand(arguments, item)
        .unwrap_err()
        .into_iter()
        .map(|error| error.to_string())
        .collect()
}

fn expanded(item: TokenStream) -> String {
    expand(TokenStream::new(), item).unwrap().to_string()
}

#[test]
fn an_impl_keeps_what_it_names_and_gains_the_other_kinds_as_unit() {
    let output = expanded(quote! {
        impl NodeExtra for Marks {
            type WordprocessingParagraph = Vec<u32>;
        }
    });
    assert!(
        output
            .starts_with("impl NodeExtra for Marks { type WordprocessingParagraph = Vec < u32 > ;")
    );
    assert!(output.contains("type WordprocessingText = () ;"));
    assert_eq!(output.matches("type WordprocessingParagraph ").count(), 1);
    assert_eq!(output.matches("type ").count(), kinds::KINDS.len());
}

#[test]
fn the_kinds_left_out_follow_in_the_order_of_the_list() {
    let output = expanded(quote! { impl NodeExtra for Marks {} });
    let expected: Vec<String> = kinds::KINDS
        .iter()
        .map(|kind| format!("type {kind} = () ;"))
        .collect();
    assert_eq!(
        output,
        format!("impl NodeExtra for Marks {{ {} }}", expected.join(" "))
    );
}

#[test]
fn generics_where_clause_and_attributes_are_kept() {
    let output = expanded(quote! {
        #[doc = "marks"]
        impl<T: Clone> NodeExtra for Marks<T> where T: Default {
            type WordprocessingParagraph = Vec<T>;
        }
    });
    assert!(output.starts_with(
        "# [doc = \"marks\"] impl < T : Clone > NodeExtra for Marks < T > where T : Default { type WordprocessingParagraph = Vec < T > ;"
    ));
}

#[test]
fn a_name_that_is_no_kind_is_refused_with_the_kind_it_nearly_is() {
    let messages = messages(
        TokenStream::new(),
        quote! { impl NodeExtra for Marks { type WordprocessingParagrap = (); } },
    );
    assert_eq!(
        messages,
        [
            "`WordprocessingParagrap` is no kind of node of officekit's typed tree; did you mean `WordprocessingParagraph`?"
        ]
    );
}

#[test]
fn a_name_that_differs_by_case_suggests_the_kind() {
    let messages = messages(
        TokenStream::new(),
        quote! { impl NodeExtra for Marks { type wordprocessingparagraph = (); } },
    );
    assert!(messages[0].ends_with("did you mean `WordprocessingParagraph`?"));
}

#[test]
fn a_name_far_from_every_kind_has_no_suggestion() {
    let messages = messages(
        TokenStream::new(),
        quote! { impl NodeExtra for Marks { type Nothing = (); } },
    );
    assert_eq!(
        messages,
        ["`Nothing` is no kind of node of officekit's typed tree"]
    );
}

#[test]
fn a_kind_named_twice_is_refused_at_the_second() {
    let messages = messages(
        TokenStream::new(),
        quote! {
            impl NodeExtra for Marks {
                type WordprocessingParagraph = ();
                type WordprocessingParagraph = ();
            }
        },
    );
    assert_eq!(messages, ["`WordprocessingParagraph` is named twice"]);
}

#[test]
fn an_item_that_is_not_a_type_is_refused() {
    let only_types =
        "`#[node_extra]` takes only `type` items: `NodeExtra` has no functions or constants";
    for member in [quote! { fn f() {} }, quote! { const N: u32 = 1; }] {
        let messages = messages(
            TokenStream::new(),
            quote! { impl NodeExtra for Marks { #member } },
        );
        assert_eq!(messages, [only_types]);
    }
}

#[test]
fn anything_but_an_impl_is_refused() {
    let messages = messages(TokenStream::new(), quote! { fn f() {} });
    assert_eq!(
        messages,
        ["`#[node_extra]` goes on an `impl` of `NodeExtra`"]
    );
}

#[test]
fn arguments_are_refused() {
    let messages = messages(quote! { all }, quote! { impl NodeExtra for Marks {} });
    assert_eq!(messages, ["`#[node_extra]` takes no arguments"]);
}
