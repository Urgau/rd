use anyhow::{anyhow, Context as _, Result};
use rustdoc_types::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use tracing::{trace, warn};

use super::render::{GlobalContext, PageContext};
use crate::pp;

pub(crate) fn item_kind2(kind: &ItemKind) -> (&'static str, bool) {
    match kind {
        ItemKind::Module => ("mod", true),
        ItemKind::Import => ("import", true),
        ItemKind::Union => ("union", true),
        ItemKind::Struct => ("struct", true),
        ItemKind::StructField => ("struct.field", false),
        ItemKind::Enum => ("enum", true),
        ItemKind::Variant => ("enum.variant", false),
        ItemKind::Function => ("fn", true),
        ItemKind::Trait => ("trait", true),
        ItemKind::TraitAlias => ("trait.alias", true),
        ItemKind::Method => ("method", false),
        ItemKind::Impl => ("impl", false),
        ItemKind::Typedef => ("typedef", true),
        ItemKind::Constant => ("const", true),
        ItemKind::Static => ("static", true),
        ItemKind::Macro => ("macro", true),
        //ItemKind::ProcMacro => ("proc.macro", true),
        ItemKind::AssocConst => ("assoc.const", false),
        ItemKind::AssocType => ("assoc.type", false),
        _ => unimplemented!(),
    }
}

pub(crate) fn item_kind(item: &Item) -> (&'static str, bool) {
    match &item.inner {
        ItemEnum::Module(_) => ("mod", true),
        ItemEnum::Import(_) => ("import", true),
        ItemEnum::Union(_) => ("union", true),
        ItemEnum::Struct(_) => ("struct", true),
        ItemEnum::StructField(_) => ("struct.field", false),
        ItemEnum::Enum(_) => ("enum", true),
        ItemEnum::Variant(_) => ("enum.variant", false),
        ItemEnum::Function(_) => ("fn", true),
        ItemEnum::Trait(_) => ("trait", true),
        ItemEnum::TraitAlias(_) => ("trait.alias", true),
        ItemEnum::Method(_) => ("method", false),
        ItemEnum::Impl(_) => ("impl", false),
        ItemEnum::Typedef(_) => ("typedef", true),
        ItemEnum::Constant(_) => ("const", true),
        ItemEnum::Static(_) => ("static", true),
        ItemEnum::Macro(_) => ("macro", true),
        ItemEnum::ProcMacro(_) => ("proc.macro", true),
        ItemEnum::AssocConst { .. } => ("assoc.const", false),
        ItemEnum::AssocType { .. } => ("assoc.type", false),
        _ => unimplemented!(),
    }
}

pub(crate) fn type_id(type_: &Type) -> Result<&Id, Option<ItemKind>> {
    match type_ {
        Type::ResolvedPath { id, .. } => Ok(id),
        Type::BorrowedRef { type_, .. } => type_id(type_),
        Type::RawPointer { type_, .. } => type_id(type_),
        Type::Slice(type_) => type_id(type_),
        Type::Array { type_, .. } => type_id(type_),
        Type::Primitive(..) => Err(Some(ItemKind::Primitive)),
        _ => Err(None),
    }
}

pub(crate) fn is_auto_trait<'krate>(krate: &'krate Crate, id: &'krate Id) -> Result<(bool, u32)> {
    let item = krate
        .index
        .get(id)
        .with_context(|| format!("Unable to find the item {:?}", id))?;

    Ok(match &item.inner {
        ItemEnum::Trait(trait_) => (trait_.is_auto, item.crate_id),
        _ => return Err(anyhow!("is_auto_trait: error not an trait")),
    })
}

pub(crate) fn id<'krate>(
    krate: &'krate Crate,
    item: &'krate Item,
) -> Option<(Cow<'krate, str>, String)> {
    if let Some(name) = &item.name {
        let (item_kind_name, is_file) = item_kind(item);

        // TODO: This seems to be another bug with the json where inner assoc type are typedef
        // whitch is clearly wrong!
        assert!(is_file || !matches!(&item.inner, ItemEnum::Typedef(_)));
        Some((Cow::Borrowed(name), format!("{}.{}", item_kind_name, name)))
    } else if let ItemEnum::Impl(impl_) = &item.inner {
        let mut name = String::new();
        let mut id = String::new();

        for token in pp::Tokens::from_item(item, &krate.index).unwrap().iter() {
            match token {
                pp::Token::Ponct(_) => id.push('-'),
                pp::Token::Ident(ident, _) => id.push_str(ident),
                pp::Token::Kw(kw) => id.push_str(kw),
                _ => {}
            }
        }

        let name_type = match &impl_.trait_ {
            Some(type_) => match type_ {
                Type::ResolvedPath { id, .. } if !id.0.starts_with("0:") => {
                    if impl_.negative {
                        name.push('!');
                    }
                    type_
                }
                _ => &impl_.for_,
            },
            None => &impl_.for_,
        };

        for token in pp::Tokens::from_type(name_type).unwrap().iter() {
            match token {
                pp::Token::Ponct(p) => name.push_str(p),
                pp::Token::Ident(ident, _) => name.push_str(ident),
                pp::Token::Kw(kw) => name.push_str(kw),
                pp::Token::Special(s) if *s == pp::SpecialToken::Space => name.push(' '),
                _ => {}
            }
        }

        Some((Cow::Owned(name), id))
    } else {
        None
    }
}

pub(crate) fn relative(base: &Path, url: &Path) -> PathBuf {
    let mut relative = PathBuf::new();

    let ends_with_html = |c: &std::path::Component| -> bool {
        match c {
            std::path::Component::Normal(path) => {
                path.to_str().map(|s| s.ends_with(".html")).unwrap_or(false)
            }
            _ => false,
        }
    };

    let mut base_components = base
        .components()
        .take_while(|c| !ends_with_html(c))
        .peekable();
    let mut url_components = url
        .components()
        .take_while(|c| !ends_with_html(c))
        .peekable();

    // Skip over the common prefix
    //while dbg!(base_components.peek().is_some()) && dbg!(base_components.peek()) == dbg!(url_components.peek()) {
    while base_components.peek().is_some() && base_components.peek() == url_components.peek() {
        base_components.next();
        url_components.next();
    }

    // Add `..` segments for the remainder of the base path
    for base_path_segment in base_components {
        // Skip empty last segments
        if let std::path::Component::Normal(s) = base_path_segment {
            if s.is_empty() {
                break;
            }
        }

        relative.push("..");
    }

    // Append the remainder of the other URI
    for url_path_segment in url_components {
        relative.push(url_path_segment);
    }

    let url_file_name = url.file_name();
    if let Some(url_file_name) = url_file_name {
        relative.push(url_file_name);
    }

    relative
}

pub(crate) fn top_of(base: &Path) -> PathBuf {
    let mut relative = PathBuf::new();

    // Add `..` segments for the remainder of the base path
    for base_path_segment in base.components() {
        // Skip empty last segments
        if let std::path::Component::Normal(s) = base_path_segment {
            if s.is_empty() || s.to_str().map(|s| s.ends_with(".html")).unwrap_or(false) {
                break;
            }
        }

        relative.push("..");
    }

    relative
}

pub(crate) fn href<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    id: &'krate Id,
) -> Option<(
    Option<&'context String>,
    PathBuf,
    Option<String>,
    &'static str,
)> {
    let to = global_context.krate.paths.get(id);

    if to.is_none() {
        if let Some(item) = global_context.krate.index.get(id) {
            match &item.inner {
                ItemEnum::Method { .. } => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("method.{}", item.name.as_ref().unwrap())),
                        "method",
                    ))
                }
                _ => warn!("not handling this kind of items"),
            }
        } else {
            warn!(?id, "id not in paths or index");
        }
        return None;
    }

    let to = to.unwrap();
    let (to_kind, to_always_file) = item_kind2(&to.kind);

    if to_always_file {
        let parts = &to.path[..(to.path.len()
            - if !matches!(to.kind, ItemKind::Module) {
                1
            } else {
                0
            })];

        let filename: PathBuf = if matches!(to.kind, ItemKind::Module) {
            "index.html".into()
        } else {
            format!("{}.{}.html", to_kind, to.path[to.path.len() - 1]).into()
        };

        let mut dest = PathBuf::with_capacity(30);
        dest.extend(parts);
        dest.push(filename);

        //debug!(?dest, ?current_filepath, ?relative);

        let (external_crate_url, path) =
            if let Some(external_crate) = global_context.krate.external_crates.get(&to.crate_id) {
                if let Some(html_root_url) = &external_crate.html_root_url {
                    (Some(html_root_url), dest)
                } else {
                    return None;
                }
            } else {
                let current_filepath = &page_context.filepath;
                (None, relative(current_filepath, &dest))
            };

        Some((external_crate_url, path, None, to_kind))
    } else {
        trace!(?to_kind, "not is_always_file");
        None
    }
}
