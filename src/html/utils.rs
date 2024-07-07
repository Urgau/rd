//! Collections of utilities functions for the html generation

use anyhow::{anyhow, Context as _, Result};
use log::{debug, trace, warn};
use rustdoc_types::*;
use std::borrow::Cow;
use std::path::{Path as StdPath, PathBuf};

use super::id::Id as HtmlId;
use super::render::{GlobalContext, PageContext};
use crate::pp;

pub(crate) fn fetch_impls<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    impls_ids: &[Id],
) -> Result<Vec<(&'krate Item, &'krate Impl, String)>> {
    let mut impls = Vec::with_capacity(impls_ids.len());

    for id in impls_ids {
        let Some(item) = global_context.krate.index.get(id) else {
            warn!("unable to find impl {:?} -- skipping", id);
            continue;
        };

        let impl_ = match &item.inner {
            ItemEnum::Impl(impl_) => impl_,
            _ => {
                return Err(anyhow::anyhow!(
                    "impl id is not impl in struct_union_content"
                ))
            }
        };

        impls.push((item, impl_, name_of(impl_)?))
    }

    impls.sort_by(|(_, _, x_name), (_, _, y_name)| x_name.cmp(y_name));
    Ok(impls)
}

pub(crate) fn prefix_item_kind(kind: &ItemKind) -> Option<(&'static str, bool)> {
    Some(match kind {
        ItemKind::Module => ("mod", true),
        ItemKind::Import => ("import", true),
        ItemKind::Union => ("union", true),
        ItemKind::Struct => ("struct", true),
        ItemKind::StructField => ("structfield", false),
        ItemKind::Enum => ("enum", true),
        ItemKind::Variant => ("variant", false),
        ItemKind::Function => ("fn", true),
        ItemKind::Trait => ("trait", true),
        ItemKind::TraitAlias => ("trait.alias", true),
        ItemKind::Impl => ("impl", false),
        ItemKind::TypeAlias => ("type", true),
        ItemKind::Constant => ("constant", true),
        ItemKind::Static => ("static", true),
        ItemKind::Macro => ("macro", true),
        ItemKind::AssocConst => ("associatedconst", false),
        ItemKind::AssocType => ("associatedtype", false),
        ItemKind::Primitive => ("primitive", true),
        ItemKind::ForeignType => return None, // TODO: not sure how to handle it
        ItemKind::ExternCrate
        | ItemKind::OpaqueTy
        | ItemKind::ProcAttribute
        | ItemKind::ProcDerive
        | ItemKind::Keyword => unreachable!(),
    })
}

pub(crate) fn prefix_item(item: &Item) -> Option<(&'static str, bool)> {
    Some(match &item.inner {
        ItemEnum::Module(_) => ("mod", true),
        ItemEnum::Import(_) => ("import", true),
        ItemEnum::Union(_) => ("union", true),
        ItemEnum::Struct(_) => ("struct", true),
        ItemEnum::StructField(_) => ("structfield", false),
        ItemEnum::Enum(_) => ("enum", true),
        ItemEnum::Variant(_) => ("variant", false),
        ItemEnum::Function(_) => ("fn", true),
        ItemEnum::Trait(_) => ("trait", true),
        ItemEnum::TraitAlias(_) => ("trait.alias", true),
        ItemEnum::Impl(_) => ("impl", false),
        ItemEnum::TypeAlias(_) => ("type", true),
        ItemEnum::Constant { .. } => ("constant", true),
        ItemEnum::Static(_) => ("static", true),
        ItemEnum::Macro(_) => ("macro", true),
        ItemEnum::ProcMacro(_) => ("proc.macro", true),
        ItemEnum::AssocConst { .. } => ("associatedconst", false),
        ItemEnum::AssocType { .. } => ("associatedtype", false),
        ItemEnum::ForeignType => return None, // TODO: not sure how to handle this
        ItemEnum::ExternCrate { .. } | ItemEnum::OpaqueTy(_) | ItemEnum::Primitive(_) => {
            unreachable!()
        }
    })
}

/// Try to get the [`Id`] of any [`Type`]
pub(crate) fn type_id(type_: &Type) -> Result<&Id, Option<ItemKind>> {
    match type_ {
        Type::ResolvedPath(Path { id, .. }) => Ok(id),
        Type::BorrowedRef { type_, .. } => type_id(type_),
        Type::RawPointer { type_, .. } => type_id(type_),
        Type::Slice(type_) => type_id(type_),
        Type::Array { type_, .. } => type_id(type_),
        Type::Primitive(..) => Err(Some(ItemKind::Primitive)),
        _ => Err(None),
    }
}

/// Determine if an [`Item`] is auto-trait and also return the crate id
pub(crate) fn is_auto_trait<'krate>(
    krate: &'krate Crate,
    id: &'krate Id,
) -> Result<Option<(bool, u32)>> {
    let Some(item) = krate.index.get(id) else {
        warn!(
            "unable to find impl (for auto-trait checking) {:?} -- skipping",
            id
        );
        return Ok(None);
    };

    Ok(match &item.inner {
        ItemEnum::Trait(trait_) => Some((trait_.is_auto, item.crate_id)),
        _ => return Err(anyhow!("is_auto_trait: error not an trait")),
    })
}

/// "Compute" a pretty-printed name for an [`Impl`]
pub(crate) fn name_of(impl_: &Impl) -> Result<String> {
    let mut name = String::new();

    // let name_type = match &impl_.trait_ {
    //     Some(type_) => match type_ {
    //         Type::ResolvedPath { id, .. } if !id.0.starts_with("0:") => {
    //             if impl_.negative {
    //                 name.push('!');
    //             }
    //             type_
    //         }
    //         _ => &impl_.for_,
    //     },
    //     None => &impl_.for_,
    // };

    for token in pp::Tokens::from_type(&impl_.for_)?.iter() {
        match token {
            pp::Token::Ponct(p) => name.push_str(p),
            pp::Token::Ident(ident, _) => name.push_str(ident),
            pp::Token::Kw(kw) => name.push_str(kw),
            pp::Token::Primitive(primitive) => name.push_str(primitive),
            pp::Token::Special(s) if *s == pp::SpecialToken::Space => name.push(' '),
            pp::Token::Special(_) => {}
            pp::Token::Attr(_) => {}
        }
    }

    Ok(name)
}

/// Compute an somewhat unique HTML-Id for a for a given [`Item`]
pub(crate) fn id<'krate>(
    krate: &'krate Crate,
    item: &'krate Item,
) -> Option<(Cow<'krate, str>, HtmlId)> {
    if let Some(name) = &item.name {
        let (item_kind_name, is_file) = prefix_item(item)?;

        // TODO: This seems to be another bug with the json where inner assoc type are typedef
        // whitch is clearly wrong!
        assert!(is_file || !matches!(&item.inner, ItemEnum::TypeAlias(_)));
        Some((
            Cow::Borrowed(name),
            HtmlId::new(format!("{}.{}", item_kind_name, name)),
        ))
    } else if let ItemEnum::Impl(impl_) = &item.inner {
        let name = name_of(impl_).ok()?;
        let mut id = String::new();

        let mut should_insert_tiret = false;
        for token in pp::Tokens::from_item(item, &krate.index).unwrap().iter() {
            match token {
                pp::Token::Ponct(_) | pp::Token::Special(pp::SpecialToken::Space) => {
                    should_insert_tiret = true
                }
                pp::Token::Ident(ident, _) => {
                    if should_insert_tiret {
                        id.push('-');
                        should_insert_tiret = false;
                    }
                    id.push_str(ident)
                }
                pp::Token::Kw(kw) => {
                    if should_insert_tiret {
                        id.push('-');
                        should_insert_tiret = false;
                    }
                    id.push_str(kw)
                }
                _ => {}
            }
        }

        Some((Cow::Owned(name), HtmlId::new(id)))
    } else {
        None
    }
}

/// Create a relative path from a base one and a target
pub(crate) fn relative(base: &StdPath, url: &StdPath) -> PathBuf {
    let mut relative = PathBuf::new();

    // TODO: This a hacky, replace with a better way
    // maybe try the url crate ?
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

/// Create a relative path for going to the top of the path
pub(crate) fn top_of(base: &StdPath) -> PathBuf {
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

/// Compute a HTML-href for a given [`Id`] in the context of the current page
pub(super) fn href<'context, 'krate>(
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
        // TODO: Here we wrongly supposed that we are in the same "page"
        if let Some(item) = global_context.krate.index.get(id) {
            match &item.inner {
                ItemEnum::Function { .. } => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("method.{}", item.name.as_ref().unwrap())),
                        "method",
                    ))
                }
                ItemEnum::AssocType { .. } => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("associatedtype.{}", item.name.as_ref().unwrap())),
                        "associatedtype",
                    ))
                }
                ItemEnum::AssocConst { .. } => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("associatedconst.{}", item.name.as_ref().unwrap())),
                        "associatedconst",
                    ))
                }
                ItemEnum::TypeAlias(..) => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("type.{}", item.name.as_ref().unwrap())),
                        "type",
                    ))
                }
                // _ => warn!("item={:?} not handling this kind of items", item),
                _ => {}
            }
        } else {
            debug!(
                "id={:?} not in paths or index (maybe a leaked private type from a reexport)",
                id
            );
        }
        return None;
    }

    let to = to.unwrap();
    let (to_kind, to_always_file) = prefix_item_kind(&to.kind)?;

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
        trace!("to_kind={:?} is not is_always_file", to_kind);
        None
    }
}

pub(crate) struct Portability<'a> {
    original: &'a str,
    inner: &'a str,
}

impl<'a> Portability<'a> {
    pub(crate) fn from_attrs<T: AsRef<str>>(attrs: &'a [T]) -> Result<Option<Self>> {
        let cfg = attrs
            .iter()
            .find(|attr| attr.as_ref().starts_with("#[cfg("));

        if cfg.is_none() {
            return Ok(None);
        }
        let cfg = cfg.unwrap().as_ref();

        let cfg_without_attr_decoration = cfg
            .get(2..cfg.len() - 1)
            .context("invalid cfg attribute: no attr decoration ?")?;
        let cfg_without_decoration = cfg_without_attr_decoration
            .get(4..cfg_without_attr_decoration.len() - 1)
            .context("invalid cfg attribute: no cfg itself ?")?;

        Ok(Some(Self {
            original: cfg,
            inner: cfg_without_decoration,
        }))
    }

    pub(crate) fn render_short(&self) -> &'a str {
        self.inner
    }

    pub(crate) fn render_long(&self) -> (&'static str, &'a str) {
        ("The portability is definied by: ", self.original)
    }
}
