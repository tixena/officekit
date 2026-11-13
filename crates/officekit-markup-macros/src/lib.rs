//! The `#[user_data]` attribute of officekit's typed tree. Use it through
//! `officekit_markup::user_data`.

extern crate alloc;

mod kinds;

use alloc::collections::BTreeSet;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::ToTokens as _;
use syn::{Error, Ident, ImplItem, ItemImpl, Result, parse_quote};

#[cfg(test)]
mod tests;

const NOT_AN_IMPL: &str = "`#[user_data]` goes on an `impl` of `UserData`";

const ONLY_TYPES: &str =
    "`#[user_data]` takes only `type` items: `UserData` has no functions or constants";

/// An `impl` of `officekit_markup::UserData` that names only the kinds it fills: every kind it
/// leaves out holds `()`.
#[proc_macro_attribute]
pub fn user_data(arguments: TokenStream, item: TokenStream) -> TokenStream {
    expand(arguments.into(), item.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// The expansion, on `proc_macro2` tokens so unit tests can run it: the `impl` as it was given,
/// then `type <Kind> = ();` for each kind it does not name, in the order of `kinds::KINDS`.
fn expand(arguments: Tokens, item: Tokens) -> Result<Tokens> {
    if !arguments.is_empty() {
        return Err(Error::new_spanned(
            arguments,
            "`#[user_data]` takes no arguments",
        ));
    }
    let mut parsed: ItemImpl = match syn::parse2(item) {
        Ok(parsed) => parsed,
        Err(error) => return Err(Error::new(error.span(), NOT_AN_IMPL)),
    };
    let mut errors: Option<Error> = None;
    let mut named: BTreeSet<String> = BTreeSet::new();
    for member in &parsed.items {
        let ImplItem::Type(kind) = member else {
            refuse(&mut errors, Error::new_spanned(member, ONLY_TYPES));
            continue;
        };
        let name = kind.ident.to_string();
        let problem = if !kinds::KINDS.contains(&name.as_str()) {
            Some(unknown_kind(&name))
        } else if named.insert(name.clone()) {
            None
        } else {
            Some(format!("`{name}` is named twice"))
        };
        if let Some(message) = problem {
            refuse(&mut errors, Error::new_spanned(&kind.ident, message));
        }
    }
    if let Some(error) = errors {
        return Err(error);
    }
    for kind in kinds::KINDS {
        if !named.contains(*kind) {
            let ident = Ident::new(kind, Span::call_site());
            parsed.items.push(parse_quote!(type #ident = ();));
        }
    }
    Ok(parsed.into_token_stream())
}

/// Add `error` to the errors found so far.
fn refuse(errors: &mut Option<Error>, error: Error) {
    match errors {
        Some(all) => all.combine(error),
        None => *errors = Some(error),
    }
}

/// The message for a name that is no kind, with the kind it was probably meant to be when one
/// differs from it only by case or by one character.
fn unknown_kind(name: &str) -> String {
    let near = kinds::KINDS
        .iter()
        .find(|kind| kind.eq_ignore_ascii_case(name))
        .or_else(|| kinds::KINDS.iter().find(|kind| within_one_edit(kind, name)));
    let message = format!("`{name}` is no kind of node of officekit's typed tree");
    match near {
        Some(kind) => format!("{message}; did you mean `{kind}`?"),
        None => message,
    }
}

/// Whether two different names are one inserted, deleted or substituted character apart.
fn within_one_edit(first: &str, second: &str) -> bool {
    let first_chars: Vec<char> = first.chars().collect();
    let second_chars: Vec<char> = second.chars().collect();
    let (short, long) = if first_chars.len() <= second_chars.len() {
        (&first_chars, &second_chars)
    } else {
        (&second_chars, &first_chars)
    };
    if long.len() - short.len() > 1 {
        return false;
    }
    let shared = short
        .iter()
        .zip(long.iter())
        .take_while(|(one, other)| one == other)
        .count();
    if long.len() == short.len() {
        shared < short.len() && short[shared + 1..] == long[shared + 1..]
    } else {
        short[shared..] == long[shared + 1..]
    }
}
