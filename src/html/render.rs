use crate::pp;
use crate::html::markdown::{Markdown, MarkdownSummaryLine, MarkdownWithToc};
use crate::html::templates::*;
use crate::html::constants::*;
use anyhow::{Context as _, Result};
use rustdoc_types::*;
use std::borrow::Cow;
use std::fs::{DirBuilder, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tracing::{debug, error, info, trace, warn};
use typed_arena::Arena;

pub(crate) struct GlobalContext<'krate> {
    pub(crate) krate: &'krate Crate,
    pub(crate) krate_name: &'krate str,
    pub(crate) files: Arena<PathBuf>,
    pub(crate) item_paths: Arena<ItemPath>,
    pub(crate) output_dir: &'krate PathBuf,
}

// TODO: Remove these pub(crate)
pub(crate) struct PageContext<'context> {
    #[allow(dead_code)]
    item: &'context Item,
    pub(crate) filepath: &'context PathBuf,
    filename: PathBuf,
    pub(crate) item_path: &'context ItemPath,
}

pub struct ItemPath(pub(crate) Vec<ItemPathComponent>);

#[derive(Clone)]
pub struct ItemPathComponent {
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) filepath: PathBuf,
}

impl<'context> ItemPath {
    fn display(
        &'context self,
        page_context: &'context PageContext<'context>,
    ) -> ItemPathDisplay<'context> {
        ItemPathDisplay(self, page_context)
    }
}

pub struct ItemPathDisplay<'a>(&'a ItemPath, &'a PageContext<'a>);

impl<'context> markup::Render for ItemPathDisplay<'context> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        for (index, item_path_component) in self.0 .0.iter().enumerate() {
            if index != 0 {
                writer.write_str("::")?;
            }
            writer.write_str("<a class=\"")?;
            writer.write_str(item_path_component.kind)?;

            writer.write_str("\" href=\"")?;
            let href = relative(self.1.filepath, &item_path_component.filepath);
            writer.write_fmt(format_args!("{}", href.display()))?;

            writer.write_str("\">")?;
            writer.write_str(&item_path_component.name)?;

            writer.write_str("</a>")?;
        }
        Ok(())
    }
}

pub struct TocSection<'toc> {
    pub(in super) name: &'static str,
    pub(in super) id: &'static str,
    pub(in super) items: Vec<(Cow<'toc, str>, TocDestination<'toc>)>,
}

pub enum TocDestination<'a> {
    Id(String),
    File(&'a PathBuf),
}

impl<'a> markup::Render for TocDestination<'a> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TocDestination::Id(id) => {
                writer.write_char('#')?;
                writer.write_str(id)
            }
            TocDestination::File(path) => {
                write!(writer, "{}", path.display())
            }
        }
    }
}




fn base_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: Option<&'context ItemPath>,
    item: &'context Item,
) -> Result<(PageContext<'context>, impl Write, &'context str)> {
    let name = item
        .name
        .as_ref()
        .context("unable to get the name of an item")?;

    let krate_path = global_context
        .krate
        .paths
        .get(&item.id)
        .with_context(|| format!("unable to find the path of for the {:?}", &item.id))?;
    let parts = &krate_path.path[..(krate_path.path.len() - 1)];

    let (item_kind_name, _item_kind_file) = item_kind(item);
    let filename: PathBuf = if matches!(krate_path.kind, ItemKind::Module) {
        format!("{}/index.html", name).into()
    } else {
        format!("{}.{}.html", item_kind_name, name).into()
    };

    match &item.inner {
        ItemEnum::Module(_) => {
            let mut path = global_context.output_dir.to_path_buf();
            path.extend(parts);
            path.push(name);

            debug!("creating the module directory {:?}", &path);
            DirBuilder::new()
                .recursive(false)
                .create(&path)
                .context("unable to create the module dir")?;
        }
        _ => {}
    }

    let mut filepath: PathBuf = "".into();
    filepath.extend(parts);
    filepath.push(&filename);

    let filepath = global_context.files.alloc(filepath);

    info!("generating {} {}", item_kind_name, name);
    debug!("creating the {} file {:?}", item_kind_name, filepath);
    trace!(?krate_path, "ID: {:?}", &item.id);

    let path = global_context.output_dir.join(&filepath);
    let file =
        File::create(&path).with_context(|| format!("unable to create the {:?} file", path))?;
    let file = BufWriter::new(file);

    Ok((
        PageContext {
            item,
            filepath,
            filename,
            item_path: global_context.item_paths.alloc({
                let mut path = vec![];
                if let Some(pip) = parent_item_path {
                    path.extend_from_slice(pip.0.as_slice());
                }
                path.push(ItemPathComponent {
                    name: name.clone(),
                    kind: item_kind_name,
                    filepath: filepath.clone(),
                });

                ItemPath(path)
            }),
        },
        file,
        name,
    ))
}

fn item_definition<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    item: &'krate Item,
) -> Result<TokensToHtml<'context, 'krate>> {
    let tokens = pp::Tokens::from_item(item, &global_context.krate.index)?;
    Ok(TokensToHtml(global_context, page_context, tokens))
}

pub(crate) fn module_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: Option<&'context ItemPath>,
    item: &'context Item,
    module: &'context Module,
) -> Result<PageContext<'context>> {
    let (page_context, mut file, module_name) = base_page(global_context, parent_item_path, item)?;

    let mut module_page_content = ModulePageContent {
        imports: Default::default(),
        modules: Default::default(),
        unions: Default::default(),
        structs: Default::default(),
        enums: Default::default(),
        functions: Default::default(),
        traits: Default::default(),
        trait_alias: Default::default(),
        typedefs: Default::default(),
        constants: Default::default(),
        macros: Default::default(),
        proc_macros: Default::default(),
    };

    let filenames = Arena::<PathBuf>::new();

    let mut toc_macros = TocSection {
        name: MACROS,
        id: MACROS_ID,
        items: Default::default(),
    };
    let mut toc_proc_macros = TocSection {
        name: PROC_MACROS,
        id: PROC_MACROS_ID,
        items: Default::default(),
    };
    let mut toc_modules = TocSection {
        name: MODULES,
        id: MODULES_ID,
        items: Default::default(),
    };
    let mut toc_unions = TocSection {
        name: UNIONS,
        id: UNIONS_ID,
        items: Default::default(),
    };
    let mut toc_structs = TocSection {
        name: STRUCTS,
        id: STRUCTS_ID,
        items: Default::default(),
    };
    let mut toc_enums = TocSection {
        name: ENUMS,
        id: ENUMS_ID,
        items: Default::default(),
    };
    let mut toc_traits = TocSection {
        name: TRAITS,
        id: TRAITS_ID,
        items: Default::default(),
    };
    let mut toc_functions = TocSection {
        name: FUNCTIONS,
        id: FUNCTIONS_ID,
        items: Default::default(),
    };
    let mut toc_typedefs = TocSection {
        name: TYPEDEFS,
        id: TYPEDEFS_ID,
        items: Default::default(),
    };
    let mut toc_constants = TocSection {
        name: CONSTANTS,
        id: CONSTANTS_ID,
        items: Default::default(),
    };

    for id in &module.items {
        let item = global_context
            .krate
            .index
            .get(id)
            .with_context(|| format!("Unable to find the item {:?}", id))?;

        if !id.0.starts_with("0:") {
            error!("ignoring for now `pub use item`: {:?}", id);
            continue;
        }

        let summary_line_doc = MarkdownSummaryLine::from_docs(
            &item.docs,
        );

        match &item.inner {
            ItemEnum::Import(_) => {
                module_page_content.imports.push((
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_item(item, &global_context.krate.index)?,
                    ),
                    Option::<String>::None,
                ));
            }
            ItemEnum::Union(union_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of union")?;

                let page_context =
                    union_page(global_context, page_context.item_path, &item, union_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_unions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.unions.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "union",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Struct(struct_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of struct")?;

                let page_context =
                    struct_page(global_context, page_context.item_path, &item, struct_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_structs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.structs.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "struct",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Enum(enum_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of enum")?;

                let page_context = enum_page(global_context, page_context.item_path, &item, enum_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_enums
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.enums.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "enum",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Function(function_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of function")?;

                let page_context =
                    function_page(global_context, &page_context.item_path, &item, function_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_functions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.functions.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "fn",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Trait(trait_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of trait")?;

                let page_context =
                    trait_page(global_context, &page_context.item_path, &item, trait_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_traits
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.traits.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "trait",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::TraitAlias(_) => {
                module_page_content.trait_alias.push((
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_item(item, &global_context.krate.index)?,
                    ),
                    Option::<String>::None,
                ));
            }
            ItemEnum::Typedef(typedef_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of typedef")?;

                let page_context2 =
                    typedef_page(global_context, &page_context.item_path, &item, typedef_)?;
                let filename = filenames.alloc(page_context2.filename);

                toc_typedefs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.typedefs.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "typedef",
                    },
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_type(&typedef_.type_)?,
                    ),
                ));
            }
            ItemEnum::Constant(constant_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of constant")?;

                let page_context =
                    constant_page(global_context, page_context.item_path, &item, constant_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_constants
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.constants.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "constant",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Macro(macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of macro")?;

                let page_context =
                    macro_page(global_context, page_context.item_path, &item, macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.macros.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "macro",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::ProcMacro(proc_macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of proc_macro")?;

                let page_context =
                    proc_macro_page(global_context, page_context.item_path, &item, proc_macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_proc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.proc_macros.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "proc_macro",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Module(module_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of module")?;
                let page_context =
                    module_page(global_context, Some(page_context.item_path), &item, module_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_modules
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.modules.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "mod",
                    },
                    summary_line_doc,
                ));
            }
            _ => todo!("don't know what to do"),
        }
    }

    let is_top_level = parent_item_path.is_none();
    let mut doc_toc = Default::default();

    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: if is_top_level { "Crate" } else { "Module" },
            item_name: module_name,
            item_path: page_context.item_path.display(&page_context),
            item_definition: Option::<String>::None,
            item_doc: MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
                &mut doc_toc,
            ),
            toc: &vec![
                toc_modules,
                toc_macros,
                toc_unions,
                toc_structs,
                toc_enums,
                toc_functions,
                toc_traits,
                toc_typedefs,
                toc_constants,
                toc_proc_macros,
            ],
            content: Some(module_page_content),
        },
    };

    writeln!(file, "{}", page)?;

    Ok(page_context)
}

fn trait_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: &'context ItemPath,
    item: &'context Item,
    trait_: &'context Trait,
) -> Result<PageContext<'context>> {
    let (page_context, mut file, name) = base_page(global_context, Some(parent_item_path), item)?;
    let definition = item_definition(global_context, &page_context, item)?;

    let mut trait_page_content = TraitPageContent {
        associated_types: Default::default(),
        associated_consts: Default::default(),
        required_methods: Default::default(),
        provided_methods: Default::default(),
        implementations_foreign_types: Default::default(),
        implementors: Default::default(),
        auto_implementors: Default::default(),
    };

    let mut toc_associated_types = TocSection {
        name: ASSOCIATED_TYPES,
        id: ASSOCIATED_TYPES_ID,
        items: vec![],
    };
    let mut toc_associated_consts = TocSection {
        name: ASSOCIATED_CONSTS,
        id: ASSOCIATED_CONSTS_ID,
        items: vec![],
    };
    let mut toc_required_methods = TocSection {
        name: REQUIRED_METHODS,
        id: REQUIRED_METHODS_ID,
        items: vec![],
    };
    let mut toc_provided_methods = TocSection {
        name: PROVIDED_METHODS,
        id: PROVIDED_METHODS_ID,
        items: vec![],
    };
    let mut toc_implementation_foreign_types = TocSection {
        name: IMPLEMENTATION_FOREIGN_TYPES,
        id: IMPLEMENTATION_FOREIGN_TYPES_ID,
        items: vec![],
    };
    let mut toc_implementors = TocSection {
        name: IMPLEMENTORS,
        id: IMPLEMENTORS_ID,
        items: vec![],
    };

    for id in &trait_.items {
        let item = global_context
            .krate
            .index
            .get(id)
            .with_context(|| format!("Unable to find the item {:?}", id))?;

        match &item.inner {
            ItemEnum::Method(method_) => {
                let (toc, who) = if method_.has_body {
                    (
                        &mut toc_provided_methods,
                        &mut trait_page_content.provided_methods,
                    )
                } else {
                    (
                        &mut toc_required_methods,
                        &mut trait_page_content.required_methods,
                    )
                };

                who.push(CodeEnchanted::from_item(
                    global_context,
                    &page_context,
                    toc,
                    &item,
                    false,
                )?);
            }
            ItemEnum::AssocConst { .. } => {
                trait_page_content
                    .associated_consts
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        &mut toc_associated_consts,
                        &item,
                        false,
                    )?);
            }
            ItemEnum::AssocType { .. } => {
                trait_page_content
                    .associated_types
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        &mut toc_associated_types,
                        &item,
                        false,
                    )?);
            }
            _ => warn!("ignore {:?}", item.inner),
        }
    }

    let impls = trait_
        .implementors
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            Ok((
                item,
                match &item.inner {
                    ItemEnum::Impl(impl_) => impl_,
                    _ => Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))?,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    for (item, impl_) in &impls {
        let (toc, who) = match type_id(&impl_.for_) {
            Ok(id) if !id.0.starts_with("0:") => (
                &mut toc_implementation_foreign_types,
                &mut trait_page_content.implementations_foreign_types,
            ),
            Err(Some(ItemKind::Primitive)) => (
                &mut toc_implementation_foreign_types,
                &mut trait_page_content.implementations_foreign_types,
            ),
            _ => (&mut toc_implementors, &mut trait_page_content.implementors),
        };

        who.push(CodeEnchantedWithExtras::from_items(
            global_context,
            &page_context,
            Some(toc),
            None,
            item,
            impl_,
            false,
        )?);
    }

    let mut doc_toc = Default::default();
    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: "Trait",
            item_name: name,
            item_definition: Some(definition),
            //item_path: &page_context.item_path,
            item_path: page_context.item_path.display(&page_context),
            item_doc: MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
                &mut doc_toc,
            ),
            toc: &vec![
                toc_associated_types,
                toc_associated_consts,
                toc_required_methods,
                toc_provided_methods,
                toc_implementation_foreign_types,
                toc_implementors,
                //toc_auto_implementors,
            ],
            content: Some(trait_page_content),
        },
    };

    writeln!(file, "{}", page)?;

    Ok(page_context)
}

macro_rules! ç {
    ($ty:ty => $fn:ident $type:literal $title:literal $fields:ident) => {
        fn $fn<'context>(
            global_context: &'context GlobalContext<'context>,
            parent_item_path: &'context ItemPath,
            item: &'context Item,
            inner: &'context $ty,
        ) -> Result<PageContext<'context>> {
            let (page_context, mut file, name) =
                base_page(global_context, Some(parent_item_path), item)?;
            let definition = item_definition(global_context, &page_context, item)?;

            let (toc, content) = struct_union_enum_content(
                global_context,
                &page_context,
                $title,
                &inner.$fields,
                &inner.impls,
            )?;

            let mut doc_toc = Default::default();
            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
                        &mut doc_toc,
                    ),
                    toc: &toc,
                    content: Some(content),
                },
            };

            writeln!(file, "{}", page)?;

            Ok(page_context)
        }
    };
}

macro_rules! é {
    ($ty:ty => $fn:ident $type:literal) => {
        fn $fn<'context>(
            global_context: &'context GlobalContext<'context>,
            parent_item_path: &'context ItemPath,
            item: &'context Item,
            #[allow(unused)] inner: &'context $ty,
        ) -> Result<PageContext<'context>> {
            let (page_context, mut file, name) = base_page(global_context, Some(parent_item_path), item)?;
            let definition = item_definition(global_context, &page_context, item)?;

            let mut doc_toc = Default::default();
            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
                        &mut doc_toc,
                    ),
                    toc: /* TODO: Optional */ &vec![],
                    content: Option::<String>::None,
                },
            };

            writeln!(file, "{}", page)?;

            Ok(page_context)
        }
    };
}

type Macro = String;

ç!(Union => union_page "Union" "Fields" fields);
ç!(Struct => struct_page "Struct" "Fields" fields);
ç!(Enum => enum_page "Enum" "Variants" variants);
é!(Typedef => typedef_page "Type Definition");
é!(Macro => macro_page "Macro");
é!(ProcMacro => proc_macro_page "Proc-Macro");
é!(Function => function_page "Function");
é!(Constant => constant_page "Constant");

impl<'context, 'krate>
    CodeEnchanted<TokensToHtml<'context, 'krate>, Markdown<'context, 'krate, 'context>>
{
    fn from_item(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        toc_section: &mut TocSection<'context>,
        item: &'krate Item,
        open: bool,
    ) -> Result<Self> {
        let id = if let Some((name, id)) = id(&global_context.krate, item) {
            toc_section
                .items
                .push((name, TocDestination::Id(id.clone())));
            Some(id)
        } else {
            None
        };

        Ok(Self {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(&item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            id,
            open,
            source_href: Option::<String>::None,
        })
    }

    fn from_item_without_id(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        item: &'krate Item,
    ) -> Result<Self> {
        Ok(Self {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(&item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            open: false,
            id: Option::<String>::None,
            source_href: Option::<String>::None,
        })
    }
}

impl<'context, 'krate>
    CodeEnchantedWithExtras<
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        Markdown<'context, 'krate, 'context>,
        CodeEnchanted<
            TokensToHtml<'context, 'krate /*, 'tokens*/>,
            Markdown<'context, 'krate, 'context>,
        >,
    >
{
    fn from_items(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        toc_top_section: Option<&mut TocSection<'context>>,
        toc_sub_section: Option<&mut TocSection<'context>>,
        item: &'krate Item,
        impl_: &'krate Impl,
        open: bool,
    ) -> Result<Self> {
        let id = if let Some(toc_top_section) = toc_top_section {
            if let Some((name, id)) = id(&global_context.krate, item) {
                toc_top_section
                    .items
                    .push((name, TocDestination::Id(id.clone())));
                Some(id)
            } else {
                None
            }
        } else {
            None
        };

        let mut toc_sub_section = toc_sub_section;

        Ok(CodeEnchantedWithExtras {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            id,
            open,
            source_href: Option::<String>::None,
            extras: impl_
                .items
                .iter()
                .map(|id| {
                    let item = global_context
                        .krate
                        .index
                        .get(id)
                        .with_context(|| format!("Unable to find the item {:?}", id))?;

                    if let Some(toc_sub_section) = &mut toc_sub_section {
                        CodeEnchanted::from_item(
                            global_context,
                            page_context,
                            toc_sub_section,
                            &item,
                            open,
                        )
                    } else {
                        CodeEnchanted::from_item_without_id(global_context, page_context, &item)
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

fn is_auto_trait<'krate>(krate: &'krate Crate, id: &'krate Id) -> Result<(bool, u32)> {
    let item = krate
        .index
        .get(id)
        .with_context(|| format!("Unable to find the item {:?}", id))?;

    Ok(match &item.inner {
        ItemEnum::Trait(trait_) => (trait_.is_auto, item.crate_id),
        _ => Err(anyhow::anyhow!("is_auto_trait: error not an trait"))?,
    })
}

fn struct_union_enum_content<'context, 'krate, 'title>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    title: &'title str,
    fields: &Vec<Id>,
    impls: &Vec<Id>,
) -> Result<(
    Vec<TocSection<'context>>,
    StructUnionEnumContent<
        'title,
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        Option<Markdown<'context, 'krate, 'context>>,
        TraitsWithItems<
            CodeEnchantedWithExtras<
                TokensToHtml<'context, 'krate /*, 'tokens*/>,
                Markdown<'context, 'krate, 'context>,
                CodeEnchanted<
                    TokensToHtml<'context, 'krate /*, 'tokens*/>,
                    Markdown<'context, 'krate, 'context>,
                >,
            >,
        >,
    >,
)> {
    let impls = impls
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            Ok((
                item,
                match &item.inner {
                    ItemEnum::Impl(impl_) => impl_,
                    _ => Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))?,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut toc_methods = TocSection {
        name: "Methods",
        id: "methods",
        items: vec![],
    };
    let mut toc_traits = TocSection {
        name: TRAIT_IMPLEMENTATIONS,
        id: TRAIT_IMPLEMENTATIONS_ID,
        items: vec![],
    };
    let mut toc_auto_traits = TocSection {
        name: AUTO_TRAIT_IMPLEMENTATIONS,
        id: AUTO_TRAIT_IMPLEMENTATIONS_ID,
        items: vec![],
    };
    let mut toc_blanket_traits = TocSection {
        name: BLANKET_IMPLEMENTATIONS,
        id: BLANKET_IMPLEMENTATIONS_ID,
        items: vec![],
    };

    // TODO: Move all the filtering logic directly in the map above
    let content = StructUnionEnumContent {
        title,
        fields: fields
            .iter()
            .map(|id| {
                let item = global_context
                    .krate
                    .index
                    .get(id)
                    .with_context(|| format!("Unable to find the item {:?}", id))?;

                Ok((
                    TokensToHtml(
                        global_context,
                        page_context,
                        pp::Tokens::from_item(&item, &global_context.krate.index)?,
                    ),
                    Markdown::from_docs(
                        global_context,
                        page_context,
                        &item.docs,
                        &item.links,
                    ),
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        traits: TraitsWithItems {
            implementations: impls
                .iter()
                .filter(|(_item, impl_)| matches!(impl_.trait_, None))
                .map(|(item, impl_)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        None,
                        Some(&mut toc_methods),
                        item,
                        impl_,
                        true,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            trait_implementations: impls
                .iter()
                .filter_map(|(item, impl_)| match (&impl_.trait_, &impl_.blanket_impl) {
                    (Some(type_), None) => match type_ {
                        Type::ResolvedPath { id, .. } => {
                            match is_auto_trait(&global_context.krate, &id) {
                                Ok((false, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    Some(&mut toc_traits),
                                    None,
                                    item,
                                    impl_,
                                    false,
                                )),
                                Err(e) => Some(Err(e)),
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Result<Vec<_>>>()?,
            auto_trait_implementations: impls
                .iter()
                .filter_map(|(item, impl_)| match (&impl_.trait_, &impl_.blanket_impl) {
                    (Some(type_), None) => match type_ {
                        Type::ResolvedPath { id, .. } => {
                            match is_auto_trait(&global_context.krate, &id) {
                                Ok((true, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    Some(&mut toc_auto_traits),
                                    None,
                                    item,
                                    impl_,
                                    false,
                                )),
                                Err(e) => Some(Err(e)),
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Result<Vec<_>>>()?,
            blanket_implementations: impls
                .iter()
                .filter(|(_item, impl_)| matches!(impl_.blanket_impl, Some(_)))
                .map(|(item, impl_)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        Some(&mut toc_blanket_traits),
                        None,
                        item,
                        impl_,
                        false,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        },
    };

    Ok((
        vec![toc_methods, toc_traits, toc_auto_traits, toc_blanket_traits],
        content,
    ))
}

fn item_kind2(kind: &ItemKind) -> (&'static str, bool) {
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

fn item_kind(item: &Item) -> (&'static str, bool) {
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

fn id<'krate>(krate: &'krate Crate, item: &'krate Item) -> Option<(Cow<'krate, str>, String)> {
    if let Some(name) = &item.name {
        let (item_kind_name, is_file) = item_kind(item);

        // TODO: This seems to be another bug with the json where inner assoc type are typedef
        // whitch is clearly wrong!
        assert_eq!(
            false,
            is_file && !matches!(&item.inner, ItemEnum::Typedef(_))
        );
        Some((Cow::Borrowed(name), format!("{}.{}", item_kind_name, name)))
    } else if let ItemEnum::Impl(impl_) = &item.inner {
        let mut name = String::new();
        let mut id = String::new();

        for token in pp::Tokens::from_item(item, &krate.index)
            .unwrap()
            .into_iter()
        {
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
                        name.push_str("!");
                    }
                    type_
                }
                _ => &impl_.for_,
            },
            None => &impl_.for_,
        };

        for token in pp::Tokens::from_type(name_type).unwrap().into_iter() {
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

fn type_id(type_: &Type) -> Result<&Id, Option<ItemKind>> {
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

pub(crate) fn relative(base: &PathBuf, url: &PathBuf) -> PathBuf {
    let mut relative = PathBuf::new();

    let ends_with_html = |c: &std::path::Component| -> bool {
        match c {
            std::path::Component::Normal(path) => path
                .to_str()
                .and_then(|s| Some(s.ends_with(".html")))
                .unwrap_or(false),
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

pub(crate) fn top_of(base: &PathBuf) -> PathBuf {
    let mut relative = PathBuf::new();

    // Add `..` segments for the remainder of the base path
    for base_path_segment in base.components() {
        // Skip empty last segments
        if let std::path::Component::Normal(s) = base_path_segment {
            if s.is_empty()
                || s.to_str()
                    .and_then(|s| Some(s.ends_with(".html")))
                    .unwrap_or(false)
            {
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
        return None;
    }
}

struct TokensToHtml<'context, 'krate>(
    &'context GlobalContext<'krate>,
    &'context PageContext<'context>,
    pp::Tokens<'krate>,
);

impl<'context, 'krate /*, 'tokens */> markup::Render
    for TokensToHtml<'context, 'krate /*, 'tokens*/>
{
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        let mut in_where_clause = false;
        for token in &*self.2 {
            match token {
                pp::Token::Ident(ident, id) => {
                    writer.write_str("<span")?;

                    if let Some(id) = id {
                        writer.write_str(" class=\"ident")?;
                        if let Some((external_crate_url, relative_path, fragment, type_of)) =
                            href(self.0, self.1, id)
                        {
                            writer.write_str(" ")?;
                            writer.write_str(type_of)?;
                            writer.write_str("\">")?;

                            writer.write_str("<a href=\"")?;
                            if let Some(external_crate_url) = external_crate_url {
                                writer.write_str(external_crate_url)?;
                                if !external_crate_url.ends_with("/") {
                                    writer.write_str("/")?;
                                }
                            }
                            writer.write_str(relative_path.to_str().unwrap())?;
                            if let Some(fragment) = fragment {
                                writer.write_str("#")?;
                                writer.write_str(&fragment)?;
                            }
                            writer.write_str("\">")?;
                            writer.write_str(ident)?;
                            writer.write_str("</a>")?;
                        } else {
                            writer.write_str("\">")?;
                            writer.write_str(ident)?;
                        }
                    } else {
                        writer.write_str(">")?;
                        writer.write_str(ident)?;
                    }

                    writer.write_str("</span>")?;
                }
                pp::Token::Kw(kw) => {
                    if *kw == "where" {
                        if in_where_clause {
                            warn!("already in where clause");
                        }
                        in_where_clause = true;
                        writer.write_str("<span class=\"where-clause\">")?;
                    }
                    writer.write_str("<span class=\"kw\">")?;
                    writer.write_str(kw)?;
                    writer.write_str("</span>")?;
                }
                pp::Token::Ponct(ponct) => {
                    writer.write_str("<span class=\"ponct\">")?;
                    match *ponct {
                        ">" => writer.write_str("&gt;")?,
                        "<" => writer.write_str("&lt;")?,
                        "&" => writer.write_str("&amp;")?,
                        _ => writer.write_str(ponct)?,
                    }
                    writer.write_str("</span>")?;
                    if *ponct == ";" && in_where_clause {
                        writer.write_str("</span>")?;
                        in_where_clause = false;
                    }
                }
                pp::Token::Special(special) => match special {
                    pp::SpecialToken::NewLine => writer.write_str("<br>")?,
                    pp::SpecialToken::Space => writer.write_str("&nbsp;")?,
                    pp::SpecialToken::Tabulation => writer.write_str("&nbsp;&nbsp;&nbsp;&nbsp;")?,
                },
                pp::Token::Plain(plain) => writer.write_str(plain)?,
            }
        }
        if in_where_clause {
            writer.write_str("</span>")?;
        }
        Ok(())
    }
}
