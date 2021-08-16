//! Pretty-printer for rustdoc-json output

use rustdoc_types::*;
use std::{collections::HashMap, fmt::Display, ops::Deref};

const ALLOWED_ATTRIBUTES: [&str; 6] = [
    "must_use",
    "export_name",
    "link_section",
    "no_mangle",
    "repr",
    "non_exhaustive",
];

#[derive(Debug, PartialEq, Clone)]
pub enum Token<'token> {
    Ident(&'token str, Option<&'token Id>),
    Kw(&'static str),
    Ponct(&'static str),
    Special(SpecialToken),
    Attr(&'token str),
}

#[derive(Debug, PartialEq, Clone)]
pub enum SpecialToken {
    NewLine,
    Space,
    Tabulation,
    Omitted,
    Ignored,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum PusherError {
    AllocationFailed,
}

trait Pusher<T> {
    fn try_push(&mut self, t: T) -> Result<(), PusherError>;
    fn try_extend_from_slice(&mut self, t: &[T]) -> Result<(), PusherError>;
}

impl<'token> Pusher<Token<'token>> for Vec<Token<'token>> {
    #[inline]
    fn try_push(&mut self, t: Token<'token>) -> Result<(), PusherError> {
        self.push(t);
        Ok(())
    }

    #[inline]
    fn try_extend_from_slice(&mut self, t: &[Token<'token>]) -> Result<(), PusherError> {
        self.extend_from_slice(t);
        Ok(())
    }
}

struct NewLineTabulationPusher<'pusher, 'token>(&'pusher mut dyn Pusher<Token<'token>>, bool);

impl<'pusher, 'token> NewLineTabulationPusher<'pusher, 'token> {
    #[inline]
    fn tabulation<F: Fn(&mut dyn Pusher<Token<'token>>) -> Result<(), FromItemErrorKind>>(
        pusher: &'pusher mut dyn Pusher<Token<'token>>,
        f: F,
    ) -> Result<(), FromItemErrorKind> {
        let mut pusher = Self(pusher, true);
        f(&mut pusher)
    }
}

impl<'pusher, 'token> Pusher<Token<'token>> for NewLineTabulationPusher<'pusher, 'token> {
    #[inline]
    fn try_push(&mut self, t: Token<'token>) -> Result<(), PusherError> {
        if self.1 {
            self.0.try_push(Token::Special(SpecialToken::Tabulation))?;
            self.1 = false;
        }
        if let Token::Special(SpecialToken::NewLine) = t {
            self.0.try_push(t)?;
            self.1 = true;
        } else {
            self.0.try_push(t)?;
            self.1 = false;
        }
        Ok(())
    }

    #[inline]
    fn try_extend_from_slice(&mut self, t: &[Token<'token>]) -> Result<(), PusherError> {
        for t in t {
            // NOTE: This is ugly but we cannot yse use IntoSlice because that would
            // make the Pusher trait not "safe" and so it will not be possible to use with dyn
            self.try_push(t.clone())?;
        }
        Ok(())
    }
}

trait IntoSlice<const N: usize> {
    type Item;

    fn into_slice(self) -> [Self::Item; N];
}

impl<'token> IntoSlice<1> for Token<'token> {
    type Item = Token<'token>;

    fn into_slice(self) -> [Self::Item; 1] {
        [self]
    }
}

impl<'token, const N: usize> IntoSlice<N> for [Token<'token>; N] {
    type Item = Token<'token>;

    fn into_slice(self) -> [Self::Item; N] {
        self
    }
}

pub struct Tokens<'tcx>(Vec<Token<'tcx>>);

impl Display for Tokens<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for token in &self.0 {
            f.write_str(match token {
                Token::Ident(s, _) => s,
                Token::Kw(s) => s,
                Token::Ponct(s) => s,
                Token::Attr(s) => s,
                Token::Special(special) => match special {
                    SpecialToken::NewLine => "\n",
                    SpecialToken::Space => " ",
                    SpecialToken::Tabulation => "    ",
                    SpecialToken::Omitted => "/* fields ommited */",
                    SpecialToken::Ignored => "...",
                },
            })?;
        }
        Ok(())
    }
}

impl<'tcx> Deref for Tokens<'tcx> {
    type Target = [Token<'tcx>];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

#[derive(Debug, PartialEq)]
pub enum FromItemErrorKind {
    InvalidItem,
    ChildrenNotFound(Id),
    UnexpectedItemType(Id, ItemKind),
    AttributeParsing,
    PusherError(PusherError),
}

impl std::fmt::Display for FromItemErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for FromItemErrorKind {}

impl From<PusherError> for FromItemErrorKind {
    fn from(push_error: PusherError) -> Self {
        Self::PusherError(push_error)
    }
}

impl Tokens<'_> {
    pub fn from_type(type_: &Type) -> Result<Tokens<'_>, FromItemErrorKind> {
        Ok({
            let mut tokens = Vec::new();

            with_type(&mut tokens, type_)?;

            Tokens(tokens)
        })
    }

    /// Get a [`Token`] from a item
    pub fn from_item<'item>(
        item: &'item Item,
        index: &'item HashMap<Id, Item>,
    ) -> Result<Tokens<'item>, FromItemErrorKind> {
        Ok(Tokens(match &item.inner {
            ItemEnum::Module(_) => {
                return Err(FromItemErrorKind::InvalidItem);
            }
            ItemEnum::ExternCrate { .. } => {
                return Err(FromItemErrorKind::InvalidItem);
            }
            ItemEnum::Import(import) => {
                let mut tokens = Vec::with_capacity(12);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;
                tokens.extend_from_slice(&[
                    Token::Kw("use"),
                    Token::Special(SpecialToken::Space),
                    Token::Ident(import.source.as_str(), import.id.as_ref()),
                ]);

                match import.source.rsplit_once("::") {
                    Some((_, name)) if name != import.name => {
                        tokens.extend_from_slice(&[
                            Token::Special(SpecialToken::Space),
                            Token::Kw("as"),
                            Token::Special(SpecialToken::Space),
                            Token::Ident(import.name.as_str(), Some(&item.id)),
                        ]);
                    }
                    _ => {}
                }

                tokens.try_push(Token::Ponct(";"))?;

                tokens
            }
            ItemEnum::Union(union_) => {
                let mut tokens = Vec::with_capacity(32);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;
                tokens.try_push(Token::Kw("union"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    &union_.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                with(
                    &mut tokens,
                    &union_.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("{"))?;

                if union_.fields_stripped {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Special(SpecialToken::Omitted))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                } else {
                    let items = union_
                        .fields
                        .iter()
                        .map(|id| match index.get(id) {
                            Some(item) => match &item.inner {
                                ItemEnum::StructField(struct_field) => Ok((item, struct_field)),
                                _ => Err(FromItemErrorKind::UnexpectedItemType(
                                    id.clone(),
                                    ItemKind::StructField,
                                )),
                            },
                            None => Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                        })
                        .collect::<Result<Vec<(_, _)>, FromItemErrorKind>>()?;

                    if !items.is_empty() {
                        NewLineTabulationPusher::tabulation(&mut tokens, |tokens| {
                            tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                            for (i, (item, struct_field)) in items.iter().enumerate() {
                                if i != 0 {
                                    tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                                }
                                with_struct_field(tokens, item, struct_field)?;
                                tokens.try_push(Token::Ponct(","))?;
                            }
                            Ok(())
                        })?;
                        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                    } else {
                        unreachable!("Union with 0 variants and non-stripped");
                    }
                }

                tokens.try_push(Token::Ponct("}"))?;
                tokens
            }
            ItemEnum::Struct(struct_) => {
                let mut tokens = Vec::with_capacity(32);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;
                tokens.try_push(Token::Kw("struct"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    &struct_.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                with(
                    &mut tokens,
                    &struct_.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                match struct_.struct_type {
                    StructType::Plain => {
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        tokens.try_push(Token::Ponct("{"))?;

                        if struct_.fields_stripped {
                            tokens.try_push(Token::Special(SpecialToken::Space))?;
                            tokens.try_push(Token::Special(SpecialToken::Omitted))?;
                            tokens.try_push(Token::Special(SpecialToken::Space))?;
                        } else {
                            // TODO: Maybe put the printing directly in the map() to avoid creating a Vec
                            let items = struct_
                                .fields
                                .iter()
                                .map(|id| match index.get(id) {
                                    Some(item) => match &item.inner {
                                        ItemEnum::StructField(struct_field) => {
                                            Ok((item, struct_field))
                                        }
                                        _ => Err(FromItemErrorKind::UnexpectedItemType(
                                            id.clone(),
                                            ItemKind::StructField,
                                        )),
                                    },
                                    None => Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                                })
                                .collect::<Result<Vec<(_, _)>, FromItemErrorKind>>()?;

                            if !items.is_empty() {
                                NewLineTabulationPusher::tabulation(&mut tokens, |tokens| {
                                    for (item, struct_field) in &items {
                                        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                                        with_struct_field(tokens, item, struct_field)?;
                                        tokens.try_push(Token::Ponct(","))?;
                                    }
                                    Ok(())
                                })?;
                                tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                            } else {
                                unreachable!("StructType::Plain with 0 fields and non-stripped");
                            }
                        }

                        tokens.try_push(Token::Ponct("}"))?;
                    }
                    StructType::Tuple => {
                        tokens.try_push(Token::Ponct("("))?;
                        if struct_.fields_stripped {
                            tokens.try_push(Token::Ponct("_"))?;
                        } else {
                            // TODO: Maybe put the printing directly in the map() to avoid creating a Vec
                            let items = struct_
                                .fields
                                .iter()
                                .map(|id| match index.get(id) {
                                    Some(item) => match &item.inner {
                                        ItemEnum::StructField(struct_field) => {
                                            Ok((item, struct_field))
                                        }
                                        _ => Err(FromItemErrorKind::UnexpectedItemType(
                                            id.clone(),
                                            ItemKind::StructField,
                                        )),
                                    },
                                    None => Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                                })
                                .collect::<Result<Vec<(_, _)>, FromItemErrorKind>>()?;

                            if !items.is_empty() {
                                for (index, (item, struct_field)) in items.iter().enumerate() {
                                    if index != 0 {
                                        tokens.try_push(Token::Ponct(","))?;
                                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                                    }
                                    //with_struct_field(&mut tokens, item, struct_field)?;
                                    with_visibility(&mut tokens, &item.visibility)?;
                                    with_type(&mut tokens, struct_field)?;
                                }
                            }
                        }
                        tokens.try_push(Token::Ponct(")"))?;
                        tokens.try_push(Token::Ponct(";"))?;
                    }
                    StructType::Unit => {
                        tokens.try_push(Token::Ponct(";"))?;
                    }
                }

                tokens
            }
            ItemEnum::StructField(struct_field) => {
                let mut tokens = Vec::with_capacity(8);

                with_struct_field(&mut tokens, item, struct_field)?;

                tokens
            }
            ItemEnum::Enum(enum_) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;
                tokens.try_push(Token::Kw("enum"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    &enum_.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                with(
                    &mut tokens,
                    &enum_.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("{"))?;

                if enum_.variants_stripped {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Special(SpecialToken::Omitted))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                } else {
                    // TODO: Maybe put the printing directly in the map() to avoid creating a Vec
                    let items = enum_
                        .variants
                        .iter()
                        .map(|id| match index.get(id) {
                            Some(item) => match &item.inner {
                                ItemEnum::Variant(variant_field) => Ok((item, variant_field)),
                                _ => Err(FromItemErrorKind::UnexpectedItemType(
                                    id.clone(),
                                    ItemKind::Variant,
                                )),
                            },
                            None => Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                        })
                        .collect::<Result<Vec<(_, _)>, FromItemErrorKind>>()?;

                    if !items.is_empty() {
                        NewLineTabulationPusher::tabulation(&mut tokens, |tokens| {
                            for (item, enum_variant) in &items {
                                tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                                with_enum_variant(tokens, index, item, enum_variant)?;
                                tokens.try_push(Token::Ponct(","))?;
                            }
                            Ok(())
                        })?;
                        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                    } else {
                        unreachable!("Enum with 0 variants and non-stripped");
                    }
                }

                tokens.try_push(Token::Ponct("}"))?;
                tokens
            }
            ItemEnum::Variant(variant) => {
                let mut tokens = Vec::with_capacity(8);

                with_enum_variant(&mut tokens, index, item, variant)?;

                tokens
            }
            ItemEnum::Function(function) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;

                if !function
                    .abi
                    .strip_prefix('\"')
                    .unwrap_or(&function.abi)
                    .strip_suffix('\"')
                    .unwrap_or(&function.abi)
                    .eq_ignore_ascii_case("rust")
                {
                    tokens.try_push(Token::Kw("extern"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(&function.abi, None))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }

                let qualifiers = &function.header;
                let qualifiers = qualifiers.iter().collect::<Vec<_>>();

                with(
                    &mut tokens,
                    &qualifiers,
                    Option::<Token>::None,
                    Some(Token::Special(SpecialToken::Space)),
                    Some(Token::Special(SpecialToken::Space)),
                    with_qualifiers,
                )?;

                tokens.try_push(Token::Kw("fn"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    without_impl(&function.generics.params),
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                tokens.try_push(Token::Ponct("("))?;
                with(
                    &mut tokens,
                    &function.decl.inputs,
                    Option::<Token>::None,
                    Option::<Token>::None,
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    |tokens, (name, ty)| {
                        tokens.try_push(Token::Ident(name, None))?;
                        tokens.try_push(Token::Ponct(":"))?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        with_type(tokens, ty)
                    },
                )?;
                tokens.try_push(Token::Ponct(")"))?;

                if let Some(output) = &function.decl.output {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ponct("-"))?;
                    tokens.try_push(Token::Ponct(">"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    with_type(&mut tokens, output)?;
                }

                if function.decl.c_variadic {
                    todo!("method c_variadic");
                }

                with(
                    &mut tokens,
                    &function.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                // tokens.try_push(Token::Ponct(";"))?;

                tokens
            }
            ItemEnum::Trait(trait_) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;

                if trait_.is_unsafe {
                    tokens.try_push(Token::Kw("unsafe"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }
                if trait_.is_auto {
                    tokens.try_push(Token::Kw("auto"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }

                tokens.try_push(Token::Kw("trait"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    &trait_.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                with(
                    &mut tokens,
                    &trait_.bounds,
                    Some([Token::Ponct(":"), Token::Special(SpecialToken::Space)]),
                    Some(Token::Special(SpecialToken::Space)), // TODO: remove ?
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_bound,
                )?;

                with(
                    &mut tokens,
                    &trait_.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("{"))?;

                NewLineTabulationPusher::tabulation(&mut tokens, |tokens| {
                    for id in &trait_.items {
                        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                        match index.get(id) {
                            Some(item) => {
                                match &item.inner {
                                    ItemEnum::AssocConst { type_, default } => {
                                        with_assoc_const(tokens, item, type_, default, false)?
                                    }
                                    ItemEnum::AssocType { bounds, default } => {
                                        with_assoc_type(tokens, item, bounds, default, false)?
                                    }
                                    ItemEnum::Method(method) => {
                                        with_method(tokens, item, method, false)?
                                    }
                                    _ => {
                                        return Err(FromItemErrorKind::UnexpectedItemType(
                                            id.clone(),
                                            /* TODO: This is wrong */ ItemKind::Trait,
                                        ));
                                    }
                                }
                            }
                            None => return Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                        }
                        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
                    }
                    Ok(())
                })?;

                tokens.try_push(Token::Ponct("}"))?;

                tokens
            }
            ItemEnum::TraitAlias(trait_alias) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;

                tokens.try_push(Token::Kw("trait"))?;
                if let Some(name) = &item.name {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ident(name, Some(&item.id)))?;
                }

                with(
                    &mut tokens,
                    &trait_alias.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;

                with(
                    &mut tokens,
                    &trait_alias.params,
                    Option::<Token>::None,
                    Option::<Token>::None,
                    Some([
                        Token::Special(SpecialToken::Space),
                        Token::Ponct("+"),
                        Token::Special(SpecialToken::Space),
                    ]),
                    with_generic_bound,
                )?;

                with(
                    &mut tokens,
                    &trait_alias.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Option::<Token>::None,
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                tokens.try_push(Token::Ponct(";"))?;
                tokens
            }
            ItemEnum::Method(method) => {
                let mut tokens = Vec::with_capacity(16);

                with_method(&mut tokens, item, method, true)?;

                tokens
            }
            ItemEnum::Impl(impl_) => {
                let mut tokens = Vec::with_capacity(32);

                with_attrs(&mut tokens, &item.attrs)?;

                if impl_.is_unsafe {
                    tokens.try_push(Token::Kw("unsafe"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }

                tokens.try_push(Token::Kw("impl"))?;
                with(
                    &mut tokens,
                    &impl_.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;

                if let Some(type_) = &impl_.trait_ {
                    if impl_.negative {
                        tokens.try_push(Token::Ponct("!"))?;
                    }
                    with_type(&mut tokens, type_)?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Kw("for"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }

                with_type(&mut tokens, &impl_.for_)?;

                with(
                    &mut tokens,
                    &impl_.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    with_where_predicate,
                )?;

                tokens
            }
            ItemEnum::Typedef(typedef) => {
                let mut tokens = Vec::with_capacity(12);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;
                tokens.extend_from_slice(&[
                    Token::Kw("type"),
                    Token::Special(SpecialToken::Space),
                    Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)),
                ]);

                with(
                    &mut tokens,
                    &typedef.generics.params,
                    Some([Token::Ponct("<")]),
                    Some(Token::Ponct(">")),
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_param_def,
                )?;

                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(&mut tokens, &typedef.type_)?;

                with(
                    &mut tokens,
                    &typedef.generics.where_predicates,
                    Some([
                        Token::Special(SpecialToken::NewLine),
                        Token::Kw("where"),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Tabulation),
                    ]),
                    Some([Token::Ponct(",")]),
                    Some([
                        Token::Ponct(","),
                        Token::Special(SpecialToken::NewLine),
                        Token::Special(SpecialToken::Space),
                    ]),
                    with_where_predicate,
                )?;

                tokens.try_push(Token::Ponct(";"))?;

                tokens
            }
            ItemEnum::OpaqueTy(_) => todo!(),
            ItemEnum::Constant(constant) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;

                tokens.try_push(Token::Kw("const"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
                tokens.try_push(Token::Ponct(":"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(&mut tokens, &constant.type_)?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(&constant.expr, None))?;
                tokens.try_push(Token::Ponct(";"))?;

                tokens
            }
            ItemEnum::Static(static_) => {
                let mut tokens = Vec::with_capacity(16);

                with_attrs(&mut tokens, &item.attrs)?;
                with_visibility(&mut tokens, &item.visibility)?;

                tokens.try_push(Token::Kw("static"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                if static_.mutable {
                    tokens.try_push(Token::Kw("mut"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }
                tokens.try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
                tokens.try_push(Token::Ponct(":"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(&mut tokens, &static_.type_)?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(&static_.expr, None))?;
                tokens.try_push(Token::Ponct(";"))?;

                tokens
            }
            ItemEnum::ForeignType => todo!(),
            ItemEnum::Macro(macro_) => {
                let mut tokens = Vec::with_capacity(12);

                with_attrs(&mut tokens, &item.attrs)?;
                // TODO: macro v1 vs macro v2
                //with_visibility(&mut tokens, &item.visibility)?;
                tokens.try_push(Token::Ident(macro_, None))?;

                tokens
            }
            ItemEnum::ProcMacro(proc_macro) => {
                let mut tokens = Vec::with_capacity(12);

                match proc_macro.kind {
                    MacroKind::Bang => {
                        tokens
                            .try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
                        tokens.try_push(Token::Ponct("!"))?;
                        tokens.try_push(Token::Ponct("("))?;
                        tokens.try_push(Token::Ponct(")"))?;
                    }
                    MacroKind::Attr => {
                        tokens.try_push(Token::Ponct("#"))?;
                        tokens.try_push(Token::Ponct("["))?;
                        tokens
                            .try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
                        tokens.try_push(Token::Ponct("]"))?;
                    }
                    MacroKind::Derive => {
                        tokens.try_push(Token::Ponct("#"))?;
                        tokens.try_push(Token::Ponct("["))?;
                        tokens.try_push(Token::Ident("derive", None))?;
                        tokens.try_push(Token::Ponct("("))?;
                        tokens
                            .try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
                        tokens.try_push(Token::Ponct(")"))?;
                        tokens.try_push(Token::Ponct("]"))?;
                    }
                }

                tokens
            }
            ItemEnum::AssocConst { type_, default } => {
                let mut tokens = Vec::with_capacity(12);

                with_assoc_const(&mut tokens, item, type_, default, true)?;

                tokens
            }
            ItemEnum::AssocType { bounds, default } => {
                let mut tokens = Vec::with_capacity(12);

                with_assoc_type(&mut tokens, item, bounds, default, true)?;

                tokens
            }
        }))
    }
}

fn with_assoc_const<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    item: &'tokens Item,
    type_: &'tokens Type,
    default: &'tokens Option<String>,
    standalone: bool,
) -> Result<(), FromItemErrorKind> {
    //with_attrs(tokens, &item.attrs)?;
    //with_visibility(&mut tokens, &item.visibility)?;

    tokens.try_push(Token::Kw("const"))?;
    tokens.try_push(Token::Special(SpecialToken::Space))?;
    tokens.try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;
    tokens.try_push(Token::Ponct(":"))?;
    tokens.try_push(Token::Special(SpecialToken::Space))?;
    with_type(tokens, type_)?;

    if let Some(default) = default {
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ponct("="))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ident(default, None))?;
    }

    if !standalone {
        tokens.try_push(Token::Ponct(";"))?;
    }

    Ok(())
}

fn with_assoc_type<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    item: &'tokens Item,
    bounds: &'tokens [GenericBound],
    default: &'tokens Option<Type>,
    standalone: bool,
) -> Result<(), FromItemErrorKind> {
    //with_attrs(tokens, &item.attrs)?;
    //with_visibility(&mut tokens, &item.visibility)?;

    tokens.try_push(Token::Kw("type"))?;
    tokens.try_push(Token::Special(SpecialToken::Space))?;
    tokens.try_push(Token::Ident(item.name.as_ref().unwrap(), Some(&item.id)))?;

    with(
        tokens,
        bounds,
        Some([Token::Ponct(":"), Token::Special(SpecialToken::Space)]),
        Option::<Token>::None,
        Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
        with_generic_bound,
    )?;

    if let Some(default) = default {
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ponct("="))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        with_type(tokens, default)?;
    }

    if !standalone {
        tokens.try_push(Token::Ponct(";"))?;
    }

    Ok(())
}

fn with_method<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    item: &'tokens Item,
    method: &'tokens Method,
    standalone: bool,
) -> Result<(), FromItemErrorKind> {
    with_attrs(tokens, &item.attrs)?;
    with_visibility(tokens, &item.visibility)?;

    if method.abi.eq_ignore_ascii_case("rust") {
        tokens.try_push(Token::Kw("extern"))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ponct("\""))?;
        tokens.try_push(Token::Ident(&method.abi, None))?;
        tokens.try_push(Token::Ponct("\""))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
    }

    let qualifiers = &method.header;
    let qualifiers = qualifiers.iter().collect::<Vec<_>>();

    with(
        tokens,
        &qualifiers,
        Option::<Token>::None,
        Some(Token::Special(SpecialToken::Space)),
        Some(Token::Special(SpecialToken::Space)),
        with_qualifiers,
    )?;

    tokens.try_push(Token::Kw("fn"))?;
    if let Some(name) = &item.name {
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ident(name, Some(&item.id)))?;
    }

    with(
        tokens,
        &method.generics.params,
        Some([Token::Ponct("<")]),
        Some(Token::Ponct(">")),
        Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
        with_generic_param_def,
    )?;

    tokens.try_push(Token::Ponct("("))?;
    if method.decl.inputs.len() <= 2 {
        with(
            tokens,
            &method.decl.inputs,
            Option::<Token>::None,
            Option::<Token>::None,
            Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
            |tokens, (name, ty)| {
                if name != "self" {
                    tokens.try_push(Token::Ident(name, None))?;
                    tokens.try_push(Token::Ponct(":"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }
                with_type(tokens, ty)
            },
        )?;
    } else {
        with(
            tokens,
            &method.decl.inputs,
            Some([
                Token::Special(SpecialToken::NewLine),
                Token::Special(SpecialToken::Tabulation),
            ]),
            Some([Token::Special(SpecialToken::NewLine)]),
            Some([
                Token::Ponct(","),
                Token::Special(SpecialToken::NewLine),
                Token::Special(SpecialToken::Tabulation),
            ]),
            |tokens, (name, ty)| {
                if name != "self" {
                    tokens.try_push(Token::Ident(name, None))?;
                    tokens.try_push(Token::Ponct(":"))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                }
                with_type(tokens, ty)
            },
        )?;
    }
    tokens.try_push(Token::Ponct(")"))?;

    if let Some(output) = &method.decl.output {
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        tokens.try_push(Token::Ponct("-"))?;
        tokens.try_push(Token::Ponct(">"))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
        with_type(tokens, output)?;
    }

    if method.decl.c_variadic {
        todo!("method c_variadic");
    }

    with(
        tokens,
        &method.generics.where_predicates,
        Some([
            Token::Special(SpecialToken::NewLine),
            Token::Kw("where"),
            Token::Special(SpecialToken::NewLine),
            Token::Special(SpecialToken::Tabulation),
        ]),
        Option::<Token>::None,
        Some([
            Token::Ponct(","),
            Token::Special(SpecialToken::NewLine),
            Token::Special(SpecialToken::Tabulation),
        ]),
        with_where_predicate,
    )?;

    if !standalone {
        if method.has_body {
            if method.generics.where_predicates.is_empty() {
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            } else {
                tokens.try_push(Token::Ponct(","))?;
                tokens.try_push(Token::Special(SpecialToken::NewLine))?;
            }
            tokens.try_push(Token::Ponct("{"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Special(SpecialToken::Ignored))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ponct("}"))?;
        } else {
            tokens.try_push(Token::Ponct(";"))?;
        }
    }

    Ok(())
}

fn with_struct_field<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    item: &'tokens Item,
    struct_field: &'tokens Type,
) -> Result<(), FromItemErrorKind> {
    with_attrs(tokens, &item.attrs)?;
    with_visibility(tokens, &item.visibility)?;

    if let Some(name) = &item.name {
        tokens.try_push(Token::Ident(name, None))?;
        tokens.try_push(Token::Ponct(":"))?;
        tokens.try_push(Token::Special(SpecialToken::Space))?;
    }
    with_type(tokens, struct_field)?;

    Ok(())
}

fn with_enum_variant<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    index: &'tokens HashMap<Id, Item>,
    item: &'tokens Item,
    enum_variant: &'tokens Variant,
) -> Result<(), FromItemErrorKind> {
    tokens.try_push(Token::Ident(item.name.as_ref().unwrap(), None))?;

    match enum_variant {
        Variant::Plain => {}
        Variant::Tuple(items) => {
            with(
                tokens,
                items,
                Some([Token::Ponct("(")]),
                Some(Token::Ponct(")")),
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                with_type,
            )?;
        }
        Variant::Struct(items) => {
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ponct("{"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;

            let items = items
                .iter()
                .map(|id| match index.get(id) {
                    Some(item) => match &item.inner {
                        ItemEnum::StructField(struct_field) => Ok((item, struct_field)),
                        _ => Err(FromItemErrorKind::UnexpectedItemType(
                            id.clone(),
                            ItemKind::StructField,
                        )),
                    },
                    None => Err(FromItemErrorKind::ChildrenNotFound(id.clone())),
                })
                .collect::<Result<Vec<(_, _)>, FromItemErrorKind>>()?;

            if !items.is_empty() {
                for (index, (item, struct_field)) in items.iter().enumerate() {
                    if index != 0 {
                        tokens.try_push(Token::Ponct(","))?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                    }
                    with_struct_field(tokens, item, struct_field)?;
                }
            } else {
                unreachable!("Enum with 0 variants and non-stripped");
            }

            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ponct("}"))?;
        }
    }

    Ok(())
}

fn with_visibility<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    visibility: &'tokens Visibility,
) -> Result<(), FromItemErrorKind> {
    match visibility {
        Visibility::Public => {
            tokens.try_push(Token::Kw("pub"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
        }
        Visibility::Default => {}
        Visibility::Crate => {
            tokens.try_push(Token::Kw("pub"))?;
            tokens.try_push(Token::Ponct("("))?;
            tokens.try_push(Token::Kw("crate"))?;
            tokens.try_push(Token::Ponct(")"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
        }
        Visibility::Restricted { parent, path } => {
            tokens.try_push(Token::Kw("pub"))?;
            tokens.try_push(Token::Ponct("("))?;
            tokens.try_push(Token::Kw("in"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ident(path, Some(parent)))?;
            tokens.try_push(Token::Ponct(")"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
        }
    }
    Ok(())
}

fn with_qualifiers<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    qualifiers: &&'tokens Qualifiers,
) -> Result<(), FromItemErrorKind> {
    match qualifiers {
        Qualifiers::Const => {
            tokens.try_push(Token::Kw("const"))?;
        }
        Qualifiers::Unsafe => {
            tokens.try_push(Token::Kw("unsafe"))?;
        }
        Qualifiers::Async => {
            tokens.try_push(Token::Kw("async"))?;
        }
        _ => {
            todo!("qualifier not handle");
        }
    }
    Ok(())
}

fn with_attrs<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    attrs: &'tokens [String],
) -> Result<(), FromItemErrorKind> {
    let mut printed = 0;

    for attr in attrs {
        let attr_name = attr
            .get(
                2..{
                    attr[2..]
                        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .ok_or(FromItemErrorKind::AttributeParsing)?
                        + 2
                },
            )
            .ok_or(FromItemErrorKind::AttributeParsing)?;
        if ALLOWED_ATTRIBUTES.contains(&attr_name) {
            if printed != 0 {
                tokens.try_push(Token::Special(SpecialToken::NewLine))?;
            }
            tokens.try_push(Token::Attr(attr))?;
            printed += 1;
        }
    }

    if printed != 0 {
        tokens.try_push(Token::Special(SpecialToken::NewLine))?;
    }
    Ok(())
}

fn with_where_predicate<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    where_predicate: &'tokens WherePredicate,
) -> Result<(), FromItemErrorKind> {
    match where_predicate {
        WherePredicate::BoundPredicate { ty, bounds } => {
            with_type(tokens, ty)?;

            tokens.try_push(Token::Ponct(":"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;

            with(
                tokens,
                bounds,
                Option::<Token>::None,
                Option::<Token>::None,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                with_generic_bound,
            )?;
        }
        WherePredicate::RegionPredicate { lifetime, bounds } => {
            tokens.try_push(Token::Ident(lifetime, None))?;
            tokens.try_push(Token::Ponct(":"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;

            with(
                tokens,
                bounds,
                Option::<Token>::None,
                Option::<Token>::None,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                with_generic_bound,
            )?;
        }
        WherePredicate::EqPredicate { lhs, rhs } => {
            with_type(tokens, lhs)?;

            tokens.try_push(Token::Ponct(":"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;

            with_type(tokens, rhs)?;
        }
    }
    Ok(())
}

fn with_generic_bound<'tokens>(
    tokens: &mut dyn Pusher<Token<'tokens>>,
    generic_bound: &'tokens GenericBound,
) -> Result<(), FromItemErrorKind> {
    match generic_bound {
        GenericBound::TraitBound {
            trait_,
            generic_params,
            modifier,
        } => {
            let modifier_str = match modifier {
                TraitBoundModifier::None => None,
                TraitBoundModifier::Maybe => Some("?"),
                TraitBoundModifier::MaybeConst => Some("?const"),
            };

            if let Some(modifier_str) = modifier_str {
                tokens.try_push(Token::Kw(modifier_str))?;
            }
            with_type(tokens, trait_)?;
            with(
                tokens,
                generic_params,
                Option::<Token>::None,
                Option::<Token>::None,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                with_generic_param_def,
            )?;
        }
        GenericBound::Outlives(n) => {
            tokens.try_push(Token::Ident(n, None))?;
        }
    }
    Ok(())
}

fn without_impl(items: &[GenericParamDef]) -> &[GenericParamDef] {
    let until = items
        .iter()
        .rev()
        .skip_while(|generic_param_def| {
            matches!(generic_param_def.kind, GenericParamDefKind::Type { .. })
                && generic_param_def.name.starts_with("impl")
        })
        .count();

    &items[..until]
}

fn with_generic_param_def<'tcx>(
    tokens: &mut dyn Pusher<Token<'tcx>>,
    generic_param_def: &'tcx GenericParamDef,
) -> Result<(), FromItemErrorKind> {
    match &generic_param_def.kind {
        GenericParamDefKind::Lifetime => {
            tokens.try_push(Token::Ident(&generic_param_def.name, None))?;
        }
        GenericParamDefKind::Type { bounds, default } => {
            if !&generic_param_def.name.starts_with("impl") {
                tokens.try_push(Token::Ident(&generic_param_def.name, None))?;

                with(
                    tokens,
                    bounds.as_slice(),
                    Some([Token::Ponct(":"), Token::Special(SpecialToken::Space)]),
                    Option::<Token>::None,
                    Some([
                        Token::Special(SpecialToken::Space),
                        Token::Ponct("+"),
                        Token::Special(SpecialToken::Space),
                    ]),
                    with_generic_bound,
                )?;
                if let Some(default) = default {
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    tokens.try_push(Token::Ponct("="))?;
                    tokens.try_push(Token::Special(SpecialToken::Space))?;
                    with_type(tokens, default)?;
                }
            }
        }
        GenericParamDefKind::Const { ty, default } => {
            tokens.try_push(Token::Kw("const"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ident(&generic_param_def.name, None))?;
            tokens.try_push(Token::Ponct(":"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            with_type(tokens, ty)?;

            if let Some(default) = default {
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(default, None))?;
            }
        }
    }
    Ok(())
}

fn with_type_binding<'tcx>(
    tokens: &mut dyn Pusher<Token<'tcx>>,
    type_bindind: &'tcx TypeBinding,
) -> Result<(), FromItemErrorKind> {
    match &type_bindind.binding {
        TypeBindingKind::Equality(type_) => {
            tokens.try_push(Token::Ident(&type_bindind.name, None))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ponct("="))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            with_type(tokens, type_)?;
        }
        TypeBindingKind::Constraint(constraint) => {
            eprintln!("don't really know how to handle TypeBindingKind::Constraint");
            with(
                tokens,
                constraint,
                Option::<Token>::None,
                Option::<Token>::None,
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                with_generic_bound,
            )?;
        }
    }
    Ok(())
}

fn with_generic_arg<'tcx>(
    tokens: &mut dyn Pusher<Token<'tcx>>,
    generic_arg: &'tcx GenericArg,
) -> Result<(), FromItemErrorKind> {
    match generic_arg {
        GenericArg::Lifetime(lifetime) => {
            tokens.try_push(Token::Ident(lifetime, None))?;
        }
        GenericArg::Type(type_) => {
            with_type(tokens, type_)?;
        }
        GenericArg::Const(constant) => {
            tokens.try_push(Token::Kw("const"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ident(&constant.expr, None))?;
            tokens.try_push(Token::Ponct(":"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            with_type(tokens, &constant.type_)?;
            if let Some(value) = &constant.value {
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("="))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(value, None))?;
            }
        }
    }
    Ok(())
}

fn with_generic_args<'tcx>(
    tokens: &mut dyn Pusher<Token<'tcx>>,
    generic_args: &'tcx GenericArgs,
) -> Result<(), FromItemErrorKind> {
    match generic_args {
        // <'a, 32, B: Copy, C = u32>
        GenericArgs::AngleBracketed { args, bindings } => {
            if !args.is_empty() || !bindings.is_empty() {
                tokens.try_push(Token::Ponct("<"))?;
                with(
                    tokens,
                    args,
                    Option::<Token>::None,
                    Option::<Token>::None,
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_generic_arg,
                )?;
                with(
                    tokens,
                    bindings,
                    if !args.is_empty() {
                        Some([Token::Ponct(","), Token::Special(SpecialToken::Space)])
                    } else {
                        None
                    },
                    Option::<Token>::None,
                    Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                    with_type_binding,
                )?;
                tokens.try_push(Token::Ponct(">"))?;
            }
        }
        // Fn(A, B) -> C
        GenericArgs::Parenthesized { inputs, output } => {
            with(
                tokens,
                inputs,
                Some([Token::Ponct("(")]),
                Some(Token::Ponct(")")),
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                with_type,
            )?;
            if let Some(output) = output {
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("-"))?;
                tokens.try_push(Token::Ponct(">"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(tokens, output)?;
            }
        }
    }
    Ok(())
}

fn with_type<'tcx>(
    tokens: &mut dyn Pusher<Token<'tcx>>,
    type_: &'tcx Type,
) -> Result<(), FromItemErrorKind> {
    match type_ {
        // Structs, enums, and traits
        Type::ResolvedPath {
            name,
            id,
            args,
            param_names,
        } => {
            if !param_names.is_empty() {
                tokens.try_push(Token::Kw("dyn"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            }
            tokens.try_push(Token::Ident(name, Some(id)))?;
            if let Some(args) = args {
                with_generic_args(tokens, args)?;
            }
            with(
                tokens,
                param_names,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                Option::<Token>::None,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                with_generic_bound,
            )?;
        }
        // Parameterized types
        Type::Generic(generic) => {
            tokens.try_push(Token::Ident(generic, None))?;
        }
        // Fixed-size numeric types (plus int/usize/float), char, arrays, slices, and tuples
        Type::Primitive(primitive) => {
            tokens.try_push(Token::Ident(primitive, None))?;
        }
        // `extern "ABI" fn`
        Type::FunctionPointer(fn_ptr) => {
            if !fn_ptr
                .abi
                .strip_prefix('\"')
                .unwrap_or(&fn_ptr.abi)
                .strip_suffix('\"')
                .unwrap_or(&fn_ptr.abi)
                .eq_ignore_ascii_case("rust")
            {
                tokens.try_push(Token::Kw("extern"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ident(&fn_ptr.abi, None))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            }

            let qualifiers = &fn_ptr.header;
            let qualifiers = qualifiers.iter().collect::<Vec<_>>();

            with(
                tokens,
                &qualifiers,
                Option::<Token>::None,
                Some(Token::Special(SpecialToken::Space)),
                Some(Token::Special(SpecialToken::Space)),
                with_qualifiers,
            )?;

            tokens.try_push(Token::Kw("fn"))?;
            with(
                tokens,
                &fn_ptr.generic_params,
                Some([Token::Ponct("<")]),
                Some(Token::Ponct(">")),
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                with_generic_param_def,
            )?;

            tokens.try_push(Token::Ponct("("))?;
            with(
                tokens,
                &fn_ptr.decl.inputs,
                Option::<Token>::None,
                Option::<Token>::None,
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                |tokens, (_, ty)| with_type(tokens, ty),
            )?;
            tokens.try_push(Token::Ponct(")"))?;

            if let Some(output) = &fn_ptr.decl.output {
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Ponct("-"))?;
                tokens.try_push(Token::Ponct(">"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(tokens, output)?;
            }
        }
        // `(String, u32, Box<usize>)`
        Type::Tuple(types) => {
            tokens.try_push(Token::Ponct("("))?;
            with(
                tokens,
                types.as_slice(),
                Some([]),
                Some([]),
                Some([Token::Ponct(","), Token::Special(SpecialToken::Space)]),
                &with_type,
            )?;
            tokens.try_push(Token::Ponct(")"))?;
        }
        // `[u32]`
        Type::Slice(type_) => {
            tokens.try_push(Token::Ponct("["))?;
            with_type(tokens, type_)?;
            tokens.try_push(Token::Ponct("]"))?;
        }
        // [u32; 15]
        Type::Array { type_, len } => {
            tokens.try_push(Token::Ponct("["))?;
            with_type(tokens, type_)?;
            tokens.try_push(Token::Ponct(";"))?;
            tokens.try_push(Token::Special(SpecialToken::Space))?;
            tokens.try_push(Token::Ident(len, None))?;
            tokens.try_push(Token::Ponct("]"))?;
        }
        // `impl TraitA + TraitB + ...`
        Type::ImplTrait(bounds) => {
            with(
                tokens,
                bounds.as_slice(),
                Some([Token::Kw("impl"), Token::Special(SpecialToken::Space)]),
                Option::<Token>::None,
                Some([
                    Token::Special(SpecialToken::Space),
                    Token::Ponct("+"),
                    Token::Special(SpecialToken::Space),
                ]),
                &with_generic_bound,
            )?;
        }
        // `!`
        Type::Never => {
            tokens.try_push(Token::Ident("!", None))?;
        }
        // `_`
        Type::Infer => {
            tokens.try_push(Token::Ident("_", None))?;
        }
        // `*mut u32`, `*u8`, etc.
        Type::RawPointer { mutable, type_ } => {
            tokens.try_push(Token::Kw("*"))?;
            if *mutable {
                tokens.try_push(Token::Kw("mut"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            }
            with_type(tokens, type_)?;
        }
        // `&'a mut String`, `&str`, etc.
        Type::BorrowedRef {
            lifetime,
            mutable,
            type_,
        } => {
            tokens.try_push(Token::Kw("&"))?;
            if let Some(lifetime) = lifetime {
                tokens.try_push(Token::Ident(lifetime, None))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            }
            if *mutable {
                tokens.try_push(Token::Kw("mut"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
            }
            with_type(tokens, type_)?;
        }
        // `<Type as Trait>::Name` or associated types like `T::Item` where `T: Iterator`
        Type::QualifiedPath {
            name,
            self_type,
            trait_,
        } => {
            if let Type::Generic(ref generic) = **self_type {
                match &**trait_ {
                    Type::ResolvedPath { args, .. } if args.is_some() => {
                        tokens.try_push(Token::Ponct("<"))?;
                        tokens.try_push(Token::Ident(generic, None))?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        tokens.try_push(Token::Kw("as"))?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        with_type(tokens, trait_)?;
                        tokens.try_push(Token::Ponct(">"))?;
                        tokens.try_push(Token::Ponct("::"))?;
                        tokens.try_push(Token::Ident(name, None))?;
                    }
                    Type::ResolvedPath { .. } => {
                        tokens.try_push(Token::Ident(generic, None))?;
                        tokens.try_push(Token::Ponct("::"))?;
                        tokens.try_push(Token::Ident(name, None))?;
                    }
                    _ => {
                        todo!("QualifiedPath: trait_ not ResolvedPath");
                    }
                }
            } else if let Type::QualifiedPath { .. } = **self_type {
                match &**trait_ {
                    Type::ResolvedPath { args, .. } if args.is_some() => {
                        tokens.try_push(Token::Ponct("<"))?;
                        with_type(tokens, self_type)?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        tokens.try_push(Token::Kw("as"))?;
                        tokens.try_push(Token::Special(SpecialToken::Space))?;
                        with_type(tokens, trait_)?;
                        tokens.try_push(Token::Ponct(">"))?;
                        tokens.try_push(Token::Ponct("::"))?;
                        tokens.try_push(Token::Ident(name, None))?;
                    }
                    Type::ResolvedPath { .. } => {
                        with_type(tokens, self_type)?;
                        tokens.try_push(Token::Ponct("::"))?;
                        tokens.try_push(Token::Ident(name, None))?;
                    }
                    _ => {
                        todo!("QualifiedPath: trait_ not ResolvedPath");
                    }
                }
            } else if let Type::ResolvedPath { .. } = **self_type {
                tokens.try_push(Token::Ponct("<"))?;
                with_type(tokens, self_type)?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                tokens.try_push(Token::Kw("as"))?;
                tokens.try_push(Token::Special(SpecialToken::Space))?;
                with_type(tokens, trait_)?;
                tokens.try_push(Token::Ponct(">"))?;
                tokens.try_push(Token::Ponct("::"))?;
                tokens.try_push(Token::Ident(name, None))?;
            } else {
                todo!("Type::QualifiedPath: not a Generic, QualifedPath or ResolvedPath");
            }
        }
    }
    Ok(())
}

fn with<
    'token,
    'item,
    I,
    Before,
    After,
    Between,
    F: Fn(&mut dyn Pusher<Token<'token>>, &'item I) -> Result<(), FromItemErrorKind>,
    const BEFORE_N: usize,
    const AFTER_N: usize,
    const BETWEEN_N: usize,
>(
    tokens: &mut dyn Pusher<Token<'token>>,
    items: &'item [I],
    before: Option<Before>, // TODO: maybe remove these Option and replace with empty slice
    after: Option<After>,
    between: Option<Between>,
    f: F,
) -> Result<(), FromItemErrorKind>
where
    Before: IntoSlice<BEFORE_N, Item = Token<'token>>,
    After: IntoSlice<AFTER_N, Item = Token<'token>>,
    Between: IntoSlice<BETWEEN_N, Item = Token<'token>> + Clone,
{
    if !items.is_empty() {
        if let Some(before) = before {
            tokens.try_extend_from_slice(&before.into_slice())?;
        }

        for (index, item) in items.iter().enumerate() {
            if index != 0 {
                if let Some(ref between) = between {
                    tokens.try_extend_from_slice(&between.clone().into_slice())?;
                }
            }

            f(tokens, item)?;
        }

        if let Some(after) = after {
            tokens.try_extend_from_slice(&after.into_slice())?;
        }
    }

    Ok(())
}
