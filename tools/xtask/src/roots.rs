use syn::{Expr, Item, Stmt, UseTree};

/// Crate roots are declarations, not a hiding place for implementation.
pub fn check_root(source: &str, binary: bool) -> Result<(), String> {
    let parsed = syn::parse_file(source).map_err(|e| e.to_string())?;
    check_attributes(&parsed.attrs)?;
    let mut mains = 0;
    for item in parsed.items {
        match item {
            Item::Mod(module) if module.content.is_none() => {
                check_attributes(&module.attrs)?;
            }
            Item::Use(import) if !matches!(import.tree, UseTree::Glob(_)) => {
                check_attributes(&import.attrs)?;
            }
            Item::ExternCrate(external) => {
                check_attributes(&external.attrs)?;
            }
            Item::Fn(function) if binary && function.sig.ident == "main" => {
                check_attributes(&function.attrs)?;
                mains += 1;
                if function.sig.asyncness.is_some()
                    || function.sig.unsafety.is_some()
                    || !function.sig.inputs.is_empty()
                    || function.block.stmts.len() != 1
                {
                    return Err(
                        "main MUST be one synchronous delegation, without parameters".into(),
                    );
                }
                let Stmt::Expr(Expr::Call(call), _) = &function.block.stmts[0] else {
                    return Err("main MUST directly call a named entrypoint".into());
                };
                let Expr::Path(entrypoint) = &*call.func else {
                    return Err("main MUST directly call a named entrypoint".into());
                };
                if !call.args.is_empty()
                    || entrypoint.qself.is_some()
                    || entrypoint
                        .path
                        .segments
                        .iter()
                        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
                {
                    return Err(
                        "main MUST delegate without constructing arguments or generic expressions"
                            .into(),
                    );
                }
            }
            _ => return Err("crate roots MUST contain declarations/re-exports only".into()),
        }
    }
    if binary && mains != 1 {
        return Err("binary root MUST contain exactly one delegating main".into());
    }
    Ok(())
}

fn check_attributes(attrs: &[syn::Attribute]) -> Result<(), String> {
    for attr in attrs {
        if !["doc", "cfg", "allow", "deny", "warn", "forbid", "path"]
            .iter()
            .any(|name| attr.path().is_ident(name))
        {
            return Err("root attributes MUST NOT expand procedural implementations".into());
        }
        if attr.path().is_ident("path") {
            let syn::Meta::NameValue(value) = &attr.meta else {
                return Err("invalid module path".into());
            };
            let Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) = &value.value
            else {
                return Err("invalid module path".into());
            };
            let path = std::path::PathBuf::from(value.value());
            if path.is_absolute()
                || path.extension().is_none_or(|s| s != "rs")
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("root module paths MUST remain inspectable local Rust files".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
