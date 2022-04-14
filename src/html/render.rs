//! HTML renderer

use anyhow::{Context as _, Result};
use log::{debug, info, trace, warn};
use rustdoc_types::*;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fs::{DirBuilder, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use typed_arena::Arena;

use super::constants::*;
use super::id::Id as HtmlId;
use super::markdown::{Markdown, MarkdownSummaryLine, MarkdownWithToc};
use super::templates::*;
use super::utils::*;
use crate::pp;

/// A context that is global for all the pages
pub(super) struct GlobalContext<'krate> {
    pub(super) opt: &'krate super::super::Opt,
    pub(super) krate: &'krate Crate,
    pub(super) krate_name: &'krate str,
    pub(super) files: Arena<PathBuf>,
    pub(super) paths: Arena<ItemPath>,
}

/// A context that is unique from each page
pub(super) struct PageContext<'context> {
    #[allow(dead_code)]
    item: &'context Item,
    pub(super) filepath: &'context PathBuf,
    pub(super) filename: PathBuf,
    pub(super) item_path: &'context ItemPath,
    pub(super) ids: Arena<HtmlId>,
}

/// Path to an item; slice of [`ItemPathComponent`]
pub(crate) struct ItemPath(pub(crate) Vec<ItemPathComponent>);

#[derive(Clone)]
pub(crate) struct ItemPathComponent {
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) filepath: PathBuf,
}

impl<'context> ItemPath {
    /// Create a `markup`able version of an [`ItemPath`]
    fn display(
        &'context self,
        page_context: &'context PageContext<'context>,
    ) -> ItemPathDisplay<'context> {
        ItemPathDisplay(self, page_context)
    }
}

struct ItemPathDisplay<'a>(&'a ItemPath, &'a PageContext<'a>);

impl<'context> markup::Render for ItemPathDisplay<'context> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        for (index, item_path_component) in self.0 .0.iter().enumerate() {
            if index != 0 {
                writer.write_str("::<wbr>")?;
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
    pub(super) name: &'static str,
    pub(super) id: &'static str,
    pub(super) items: Vec<(Cow<'toc, str>, TocDestination<'toc>)>,
}

enum TocSupplier<Supply> {
    Top(Supply),
    Sub(Supply, Supply, Supply),
}

pub enum TocDestination<'a> {
    Id(&'a HtmlId),
    File(&'a PathBuf),
}

impl<'a> markup::Render for TocDestination<'a> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TocDestination::Id(id) => {
                write!(writer, "{}", id.with_pound())
            }
            TocDestination::File(path) => {
                write!(writer, "{}", path.display())
            }
        }
    }
}

impl<'deprecation> DeprecationNotice<'deprecation> {
    fn from(deprecation: &'deprecation Option<Deprecation>) -> Option<Self> {
        deprecation.as_ref().map(|deprecation| Self {
            since: &deprecation.since,
            note: &deprecation.note,
        })
    }
}

impl<'portability> PortabilityNotice<'portability> {
    fn from<T: AsRef<str>>(attrs: &'portability [T]) -> Result<Option<Self>> {
        Ok(Portability::from_attrs(attrs)?
            .as_ref()
            .map(Portability::render_long)
            .map(|(message, portability)| Self {
                message,
                portability,
            }))
    }
}

fn dump_to<P: AsRef<std::path::Path>>(path: P, buf: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    std::io::Write::write_all(&mut file, buf)?;
    Ok(())
}

pub(crate) fn render_global(
    opt: &super::super::Opt,
    _outputs: &[PathBuf],
) -> Result<PathBuf> {

    // TODO: Do a global index with the outputs links

    dump_to(
        format!("{}/{}", &opt.output.display(), STYLE_CSS),
        include_bytes!("static/css/style.css"),
    )?;
    dump_to(
        format!("{}/{}", &opt.output.display(), RUST_SVG),
        include_bytes!("static/imgs/rust.svg"),
    )?;
    dump_to(
        format!("{}/{}", &opt.output.display(), SEARCH_JS),
        include_bytes!("static/js/search.js"),
    )?;

    Ok(opt.output.clone())
}

/// Html rendering entry
pub(crate) fn render<'krate>(
    opt: &super::super::Opt,
    krate: &'krate Crate,
    krate_item: &'krate Item,
) -> Result<PathBuf> {
    if let ItemEnum::Module(krate_module) = &krate_item.inner {
        let mut global_context = GlobalContext {
            opt,
            krate,
            files: Default::default(),
            paths: Default::default(),
            krate_name: krate_item.name.as_ref().context("expect a crate name")?,
        };

        let module_page_context = module_page(&global_context, None, krate_item, krate_module)?;
        let module_index_path = global_context.opt.output.join(module_page_context.filepath);
        let mut search = String::new();

        search.push_str("\n\nconst INDEX = JSON.parse('[");
        for (iitem, item) in global_context.paths.iter_mut().enumerate() {
            if iitem != 0 {
                search.push(',');
            }
            search.push_str("{\"components\":[");
            for (icomponent, component) in item.0.iter().enumerate() {
                if icomponent != 0 {
                    search.push(',');
                }
                search.push_str("{\"name\":\"");
                search.push_str(&component.name);
                search.push_str("\",\"lower_case_name\":\"");
                search.push_str(&component.name.to_ascii_lowercase());
                search.push_str("\",\"kind\":\"");
                search.push_str(component.kind);
                search.push_str("\"}");
            }

            let last = item.0.last().unwrap();
            search.push_str("],\"filepath\":\"");
            search.push_str(&format!("{}", last.filepath.display()));
            search.push_str("\"}");
        }
        search.push_str("]');\n");

        dump_to(
            format!(
                "{}/{}/{}",
                &opt.output.display(),
                &krate_item.name.as_ref().unwrap(),
                SEARCH_INDEX_JS,
            ),
            search.as_bytes(),
        )?;

        Ok(module_index_path)
    } else {
        anyhow::bail!("main item is not a Module")
    }
}

/// Entry point of each page that create the file, page_context, ...
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

    let (item_kind_name, _item_kind_file) =
        prefix_item(item).context("unable to get of this item")?;
    let filename: PathBuf = if matches!(krate_path.kind, ItemKind::Module) {
        format!("{}/index.html", name).into()
    } else {
        format!("{}.{}.html", item_kind_name, name).into()
    };

    if let ItemEnum::Module(_) = &item.inner {
        let mut path = global_context.opt.output.to_path_buf();
        path.extend(parts);
        path.push(name);

        debug!("creating the module directory {:?}", &path);
        DirBuilder::new()
            .recursive(false)
            .create(&path)
            .context("unable to create the module dir")?;
    }

    let mut filepath: PathBuf = "".into();
    filepath.extend(parts);
    filepath.push(&filename);

    let filepath = global_context.files.alloc(filepath);

    info!("generating {} {}", item_kind_name, name);
    debug!("creating the {} file {:?}", item_kind_name, filepath);
    trace!("ID: {:?} -- krate_path {:?}", &item.id, &krate_path);

    let path = global_context.opt.output.join(&filepath);
    let file =
        File::create(&path).with_context(|| format!("unable to create the {:?} file", path))?;
    let file = BufWriter::new(file);

    Ok((
        PageContext {
            item,
            filepath,
            filename,
            item_path: global_context.paths.alloc({
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
            ids: Default::default(),
        },
        file,
        name,
    ))
}

/// Helper function to get the item definition in a `markup`able way
fn item_definition<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    item: &'krate Item,
) -> Result<TokensToHtml<'context, 'krate>> {
    let tokens = pp::Tokens::from_item(item, &global_context.krate.index)?;
    Ok(TokensToHtml(global_context, page_context, tokens))
}

/// Module page generation function
fn module_page<'context>(
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

    // TODO: this could probably be removed
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

    let mut items = module
        .items
        .iter()
        .filter_map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))
                .ok()?;

            if !id.0.starts_with("0:") {
                warn!("ignoring for now `pub use`: {:?}", id);
                return None;
            }

            Some(Ok(item))
        })
        .collect::<Result<Vec<_>>>()?;
    items.sort_by(|x_item, y_item| match (&x_item.inner, &y_item.inner) {
        (ItemEnum::Module(_), ItemEnum::Module(_)) => x_item.name.cmp(&y_item.name),
        (ItemEnum::Module(_), _) => Ordering::Less,
        (_, ItemEnum::Module(_)) => Ordering::Greater,
        _ => x_item.name.cmp(&y_item.name),
    });

    for item in items {
        let summary =
            MarkdownSummaryLine::from_docs(global_context, &page_context, &item.docs, &item.links);
        let portability = Portability::from_attrs(&item.attrs)?
            .as_ref()
            .map(Portability::render_short);
        let deprecated = item.deprecation.as_ref().map(|d| match d.since {
            Some(ref since) if since != "none" => "Deprecated",
            _ => "Future deprecation",
        });
        let unsafety = Option::<&str>::None;

        match &item.inner {
            ItemEnum::Import(_) => {
                module_page_content.imports.push(ModuleSectionItem {
                    name: InlineCode {
                        code: TokensToHtml(
                            global_context,
                            &page_context,
                            pp::Tokens::from_item(item, &global_context.krate.index)?,
                        ),
                    },
                    summary: Option::<String>::None,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Union(union_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of union")?;

                let page_context =
                    union_page(global_context, page_context.item_path, item, union_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_unions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.unions.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "union",
                    },
                    unsafety,
                    summary,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Struct(struct_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of struct")?;

                let page_context =
                    struct_page(global_context, page_context.item_path, item, struct_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_structs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.structs.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "struct",
                    },
                    unsafety,
                    summary,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Enum(enum_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of enum")?;

                let page_context = enum_page(global_context, page_context.item_path, item, enum_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_enums
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.enums.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "enum",
                    },
                    unsafety,
                    summary,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Function(function_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of function")?;

                let page_context =
                    function_page(global_context, page_context.item_path, item, function_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_functions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.functions.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "fn",
                    },
                    summary,
                    deprecated,
                    portability,
                    unsafety: if function_.header.unsafe_ {
                        Some("This function is unsafe to use")
                    } else {
                        unsafety
                    }
                });
            }
            ItemEnum::Trait(trait_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of trait")?;

                let page_context =
                    trait_page(global_context, page_context.item_path, item, trait_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_traits
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.traits.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "trait",
                    },
                    summary,
                    deprecated,
                    portability,
                    unsafety: if trait_.is_unsafe {
                        Some("This trait is unsafe to use")
                    } else {
                        unsafety
                    }
                });
            }
            ItemEnum::TraitAlias(_) => {
                module_page_content.trait_alias.push(ModuleSectionItem {
                    name: InlineCode {
                        code: TokensToHtml(
                            global_context,
                            &page_context,
                            pp::Tokens::from_item(item, &global_context.krate.index)?,
                        ),
                    },
                    summary: Option::<String>::None,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Typedef(typedef_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of typedef")?;

                let page_context2 =
                    typedef_page(global_context, page_context.item_path, item, typedef_)?;
                let filename = filenames.alloc(page_context2.filename);

                toc_typedefs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));

                enum Either<Left, Right> {
                    Left(Left),
                    Right(Right),
                }

                impl<Left: markup::Render, Right: markup::Render> markup::Render for Either<Left, Right> {
                    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
                        match self {
                            Either::Left(left) => markup::Render::render(left, writer),
                            Either::Right(right) => markup::Render::render(right, writer),
                        }
                    }
                }

                module_page_content.typedefs.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "type",
                    },
                    summary: if let Some(summary_line_doc) = summary {
                        Either::Left(summary_line_doc)
                    } else {
                        Either::Right(InlineCode {
                            code: TokensToHtml(
                                global_context,
                                &page_context,
                                pp::Tokens::from_type(&typedef_.type_)?,
                            ),
                        })
                    },
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Constant(constant_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of constant")?;

                let page_context =
                    constant_page(global_context, page_context.item_path, item, constant_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_constants
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.constants.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "constant",
                    },
                    summary,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Static(static_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of constant")?;

                let page_context =
                    static_page(global_context, page_context.item_path, item, static_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_constants
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.constants.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "static",
                    },
                    summary,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Macro(macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of macro")?;

                let page_context =
                    macro_page(global_context, page_context.item_path, item, macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.macros.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "macro",
                    },
                    summary,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::ProcMacro(proc_macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of proc_macro")?;

                let page_context =
                    proc_macro_page(global_context, page_context.item_path, item, proc_macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_proc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.proc_macros.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "proc_macro",
                    },
                    summary,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            ItemEnum::Module(module_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of module")?;
                let page_context =
                    module_page(global_context, Some(page_context.item_path), item, module_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_modules
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.modules.push(ModuleSectionItem {
                    name: ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "mod",
                    },
                    summary,
                    unsafety,
                    deprecated,
                    portability,
                });
            }
            _ => unreachable!("module item shouldn't have a this type of item"),
        }
    }

    let is_top_level = parent_item_path.is_none();
    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: if is_top_level { "Crate" } else { "Module" },
            item_name: module_name,
            item_path: page_context.item_path.display(&page_context),
            item_deprecation: DeprecationNotice::from(&item.deprecation),
            item_portability: PortabilityNotice::from(&item.attrs)?,
            item_definition: Option::<String>::None,
            item_doc: MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
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

/// Function for generating a Trait page
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

    let mut items = trait_
        .items
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            Ok((
                item,
                item.name.as_ref().context("missing name for trait item")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    items.sort_by(|(_, x_name), (_, y_name)| x_name.cmp(y_name));

    for (item, _name) in items {
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
                    None,
                    Some(toc),
                    item,
                    true,
                )?);
            }
            ItemEnum::AssocConst { .. } => {
                trait_page_content
                    .associated_consts
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        None,
                        Some(&mut toc_associated_consts),
                        item,
                        true,
                    )?);
            }
            ItemEnum::AssocType { .. } => {
                trait_page_content
                    .associated_types
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        None,
                        Some(&mut toc_associated_types),
                        item,
                        true,
                    )?);
            }
            _ => warn!("ignore {:?}", item.inner),
        }
    }

    let mut impls = trait_
        .implementations
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            let impl_ = match &item.inner {
                ItemEnum::Impl(impl_) => impl_,
                _ => {
                    return Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))
                }
            };

            Ok((item, impl_, name_of(impl_)?))
        })
        .collect::<Result<Vec<_>>>()?;
    impls.sort_by(|(_, _, x_name), (_, _, y_name)| x_name.cmp(y_name));

    for (item, impl_, _name) in &impls {
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
            TocSupplier::Top(toc),
            item,
            impl_,
            false,
        )?);
    }

    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: "Trait",
            item_name: name,
            item_definition: Some(definition),
            item_deprecation: DeprecationNotice::from(&item.deprecation),
            item_portability: PortabilityNotice::from(&item.attrs)?,
            item_path: page_context.item_path.display(&page_context),
            item_doc: MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            toc: &vec![
                toc_associated_types,
                toc_associated_consts,
                toc_required_methods,
                toc_provided_methods,
                toc_implementation_foreign_types,
                toc_implementors,
            ],
            content: Some(trait_page_content),
        },
    };

    writeln!(file, "{}", page)?;

    Ok(page_context)
}

/// Function for generating the content of an struct, union or enum
fn struct_union_enum_content<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    title: &'static str,
    variants: &[Id],
    impls: &[Id],
) -> Result<(Vec<TocSection<'context>>, impl markup::Render + 'context)> {
    let mut impls = impls
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            let impl_ = match &item.inner {
                ItemEnum::Impl(impl_) => impl_,
                _ => {
                    return Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))
                }
            };

            Ok((item, impl_, name_of(impl_)?))
        })
        .collect::<Result<Vec<_>>>()?;
    impls.sort_by(|(_, _, x_name), (_, _, y_name)| x_name.cmp(y_name));

    let mut toc_variants = TocSection {
        name: VARIANTS,
        id: VARIANTS_ID,
        items: vec![],
    };
    let mut toc_methods = TocSection {
        name: METHODS,
        id: METHODS_ID,
        items: vec![],
    };
    let mut toc_assoc_types = TocSection {
        name: ASSOCIATED_TYPES,
        id: ASSOCIATED_TYPES_ID,
        items: vec![],
    };
    let mut toc_assoc_consts = TocSection {
        name: ASSOCIATED_CONSTS,
        id: ASSOCIATED_CONSTS_ID,
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
        variants: variants
            .iter()
            .map(|id| {
                let item = global_context
                    .krate
                    .index
                    .get(id)
                    .with_context(|| format!("Unable to find the item {:?}", id))?;

                VariantEnchantedWithExtras::from_variant(
                    global_context,
                    page_context,
                    &mut toc_variants,
                    item,
                )
            })
            .collect::<Result<Vec<_>>>()?,
        traits: TraitsWithItems {
            implementations: impls
                .iter()
                .filter(|(_, impl_, _)| matches!(impl_.trait_, None))
                .map(|(item, impl_, _)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        TocSupplier::Sub(
                            &mut toc_methods,
                            &mut toc_assoc_types,
                            &mut toc_assoc_consts,
                        ),
                        item,
                        impl_,
                        true,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            trait_implementations: impls
                .iter()
                .filter_map(
                    |(item, impl_, _)| match (&impl_.trait_, &impl_.blanket_impl) {
                        (Some(Type::ResolvedPath { id, .. }), None) => {
                            match is_auto_trait(global_context.krate, id) {
                                Ok((false, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    TocSupplier::Top(&mut toc_traits),
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
                )
                .collect::<Result<Vec<_>>>()?,
            auto_trait_implementations: impls
                .iter()
                .filter_map(
                    |(item, impl_, _)| match (&impl_.trait_, &impl_.blanket_impl) {
                        (Some(Type::ResolvedPath { id, .. }), None) => {
                            match is_auto_trait(global_context.krate, id) {
                                Ok((true, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    TocSupplier::Top(&mut toc_auto_traits),
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
                )
                .collect::<Result<Vec<_>>>()?,
            blanket_implementations: impls
                .iter()
                .filter(|(_item, impl_, _)| matches!(impl_.blanket_impl, Some(_)))
                .map(|(item, impl_, _)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        TocSupplier::Top(&mut toc_blanket_traits),
                        item,
                        impl_,
                        false,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        },
    };

    Ok((
        vec![
            toc_variants,
            toc_methods,
            toc_assoc_types,
            toc_assoc_consts,
            toc_traits,
            toc_auto_traits,
            toc_blanket_traits,
        ],
        content,
    ))
}

macro_rules! ç {
    ($ty:ty => $fn:ident $type:literal $title:literal $fields:ident) => {
        /// Function for generating a $ty page
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

            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_portability: PortabilityNotice::from(&item.attrs)?,
                    item_deprecation: DeprecationNotice::from(&item.deprecation),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
                    ),
                    toc: &toc,
                    content: Some(content),
                },
            };

            writeln!(file, "{}", page)?;
            drop(page);

            Ok(page_context)
        }
    };
}

macro_rules! é {
    ($ty:ty => $fn:ident $type:literal) => {
        /// Function for generating a $ty page
        fn $fn<'context>(
            global_context: &'context GlobalContext<'context>,
            parent_item_path: &'context ItemPath,
            item: &'context Item,
            #[allow(unused)] inner: &'context $ty,
        ) -> Result<PageContext<'context>> {
            let (page_context, mut file, name) = base_page(global_context, Some(parent_item_path), item)?;
            let definition = item_definition(global_context, &page_context, item)?;

            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_portability: PortabilityNotice::from(&item.attrs)?,
                    item_deprecation: DeprecationNotice::from(&item.deprecation),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
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

ç!(Union => union_page "Union" "Fields" fields);
ç!(Struct => struct_page "Struct" "Fields" fields);
ç!(Enum => enum_page "Enum" "Variants" variants);
é!(Typedef => typedef_page "Type Definition");
é!(str => macro_page "Macro");
é!(ProcMacro => proc_macro_page "Proc-Macro");
é!(Function => function_page "Function");
é!(Constant => constant_page "Constant");
é!(Static => static_page "Static");

impl<'context, 'krate>
    CodeEnchanted<
        TokensToHtml<'context, 'krate>,
        Markdown<'context, 'krate, 'context>,
        DeprecationNotice<'context>,
        &'context HtmlId,
    >
{
    fn from_item(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        parent_id: Option<&HtmlId>,
        toc_section: Option<&mut TocSection<'context>>,
        item: &'krate Item,
        open: bool,
    ) -> Result<Self> {
        let id = if let Some((name, mut id)) = id(global_context.krate, item) {
            if let Some(parent_id) = parent_id {
                id = parent_id + id;
            }
            let id = page_context.ids.alloc(id);

            if let Some(toc_section) = toc_section {
                toc_section.items.push((name, TocDestination::Id(id)));
            }
            Some(&*id)
        } else {
            None
        };

        Ok(Self {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(global_context, page_context, id, &item.docs, &item.links),
            deprecation: DeprecationNotice::from(&item.deprecation),
            id,
            open,
            source_href: Option::<String>::None,
        })
    }
}

impl<'context, 'krate>
    CodeEnchantedWithExtras<
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        Markdown<'context, 'krate, 'context>,
        DeprecationNotice<'context>,
        &'context HtmlId,
        CodeEnchanted<
            TokensToHtml<'context, 'krate /*, 'tokens*/>,
            Markdown<'context, 'krate, 'context>,
            DeprecationNotice<'context>,
            &'context HtmlId,
        >,
    >
{
    fn from_items(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        mut toc_section: TocSupplier<&mut TocSection<'context>>,
        item: &'krate Item,
        impl_: &'krate Impl,
        open: bool,
    ) -> Result<Self> {
        let parent_id = if let TocSupplier::Top(toc_top_section) = &mut toc_section {
            if let Some((name, id)) = id(global_context.krate, item) {
                let id = page_context.ids.alloc(id);

                toc_top_section.items.push((name, TocDestination::Id(id)));
                Some(&*id)
            } else {
                None
            }
        } else {
            None
        };

        Ok(CodeEnchantedWithExtras {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(
                global_context,
                page_context,
                match toc_section {
                    TocSupplier::Top(_) => parent_id,
                    _ => None,
                },
                &item.docs,
                &item.links,
            ),
            deprecation: DeprecationNotice::from(&item.deprecation),
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

                    CodeEnchanted::from_item(
                        global_context,
                        page_context,
                        parent_id,
                        if let TocSupplier::Sub(toc_methods, toc_assoc_types, toc_assoc_consts) =
                            &mut toc_section
                        {
                            Some(match item.inner {
                                ItemEnum::Method(_) => toc_methods,
                                ItemEnum::AssocConst { .. } => toc_assoc_consts,
                                ItemEnum::AssocType { .. } => toc_assoc_types,
                                _ => unreachable!("cannot be anything else"),
                            })
                        } else {
                            None
                        },
                        item,
                        open,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            id: parent_id,
        })
    }
}

impl<'context, 'krate>
    VariantEnchanted<
        &'context HtmlId,
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        Markdown<'context, 'krate, 'context>,
        DeprecationNotice<'context>,
    >
{
    fn from_type(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        parent_id: &HtmlId,
        type_: &'krate Type,
        pos: usize,
    ) -> Result<Self> {
        let id = HtmlId::new(format!("field.{}", pos));
        let id = page_context.ids.alloc(parent_id + id);

        Ok(Self {
            def: TokensToHtml(global_context, page_context, pp::Tokens::from_type(type_)?),
            id,
            doc: None,
            deprecation: None,
        })
    }

    fn from_item(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        parent_id: &'context HtmlId,
        item: &'krate Item,
    ) -> Result<Self> {
        let (_, id) = id(global_context.krate, item).context("TODO")?;
        let id = page_context.ids.alloc(parent_id + id);

        Ok(Self {
            def: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            id,
            doc: Markdown::from_docs(
                global_context,
                page_context,
                Some(parent_id),
                &item.docs,
                &item.links,
            ),
            deprecation: DeprecationNotice::from(&item.deprecation),
        })
    }
}

impl<'context, 'krate>
    VariantEnchantedWithExtras<
        &'context HtmlId,
        TokensToHtml<'context, 'krate>,
        Markdown<'context, 'krate, 'context>,
        DeprecationNotice<'context>,
        VariantEnchanted<
            &'context HtmlId,
            TokensToHtml<'context, 'krate /*, 'tokens*/>,
            Markdown<'context, 'krate, 'context>,
            DeprecationNotice<'context>,
        >,
    >
{
    fn from_variant(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        toc_section: &mut TocSection<'context>,
        item: &'krate Item,
    ) -> Result<Self> {
        let parent_id = if let Some((name, id)) = id(global_context.krate, item) {
            let id = page_context.ids.alloc(id);
            toc_section.items.push((name, TocDestination::Id(id)));
            &*id
        } else {
            unreachable!()
        };

        Ok(VariantEnchantedWithExtras {
            id: &*parent_id,
            def: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            doc: Markdown::from_docs(
                global_context,
                page_context,
                Some(parent_id),
                &item.docs,
                &item.links,
            ),
            deprecation: DeprecationNotice::from(&item.deprecation),
            extras: match &item.inner {
                ItemEnum::Variant(v) => match v {
                    Variant::Struct(ids) => Some(
                        ids.iter()
                            .map(|id| {
                                let item =
                                    global_context.krate.index.get(id).with_context(|| {
                                        format!("Unable to find the item {:?}", id)
                                    })?;

                                VariantEnchanted::from_item(
                                    global_context,
                                    page_context,
                                    parent_id,
                                    item,
                                )
                            })
                            .collect::<Result<Vec<_>>>()?,
                    ),
                    Variant::Tuple(types) => Some(
                        types
                            .iter()
                            .enumerate()
                            .map(|(pos, type_)| {
                                VariantEnchanted::from_type(
                                    global_context,
                                    page_context,
                                    parent_id,
                                    type_,
                                    pos,
                                )
                            })
                            .collect::<Result<Vec<_>>>()?,
                    ),
                    Variant::Plain => None,
                },
                ItemEnum::StructField(_) => None,
                _ => unreachable!(),
            },
        })
    }
}

/// Convert a [`pp::Tokens`] struct to an `markup`able output
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
                                if !external_crate_url.ends_with('/') {
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
                    if (*ponct == ";" || *ponct == "{") && in_where_clause {
                        writer.write_str("</span>")?;
                        in_where_clause = false;
                    }
                    writer.write_str("<span class=\"ponct\">")?;
                    match *ponct {
                        ">" => writer.write_str("&gt;")?,
                        "<" => writer.write_str("&lt;")?,
                        "&" => writer.write_str("&amp;")?,
                        _ => writer.write_str(ponct)?,
                    }
                    writer.write_str("</span>")?;
                }
                pp::Token::Attr(attr) => {
                    writer.write_str("<span class=\"attr\">")?;
                    writer.write_str(attr)?;
                    writer.write_str("</span>")?;
                }
                pp::Token::Primitive(primitive) => {
                    writer.write_str("<span class=\"primitive\">")?;
                    writer.write_str(primitive)?;
                    writer.write_str("</span>")?;
                }
                pp::Token::Special(special) => match special {
                    pp::SpecialToken::NewLine => writer.write_str("<br>")?,
                    pp::SpecialToken::Space => writer.write_str("&nbsp;")?,
                    pp::SpecialToken::Tabulation => writer.write_str("&nbsp;&nbsp;&nbsp;&nbsp;")?,
                    pp::SpecialToken::Hidden { all: true } => {
                        writer.write_str("/* fields hidden */")?
                    }
                    pp::SpecialToken::Hidden { all: false } => {
                        writer.write_str("/* some fields hidden */")?
                    }
                    pp::SpecialToken::Ignored => writer.write_str("...")?,
                },
            }
        }
        if in_where_clause {
            writer.write_str("</span>")?;
        }
        Ok(())
    }
}
