//! §6.21: accepting an implicit static initializer preserves static lifetime.
//! Normalize accepted declarations before instance rewriting and bytecode
//! preparation, so all execution paths see the same explicit lifetime.

use super::{collect_expr_idents, relax_implicit_static};
use crate::SourceDefinition;
use crate::ast::decl::{ClassDeclaration, ClassItem, ClassMethodKind, ModuleItem, PackageItem};
use crate::ast::stmt::{ForInit, Statement, StatementKind};
use crate::ast::types::Lifetime;
use crate::hasher::{HashMap, HashSet};
use std::rc::Rc;

pub(crate) fn normalize(definitions: &mut HashMap<String, SourceDefinition>) {
    if !relax_implicit_static() {
        return;
    }
    for definition in definitions.values_mut() {
        match definition {
            SourceDefinition::Module(m) => {
                let m = Rc::make_mut(m);
                items(
                    &mut m.items,
                    m.lifetime != Some(Lifetime::Automatic),
                    &m.name.name,
                );
            }
            SourceDefinition::Interface(m) => {
                let m = Rc::make_mut(m);
                items(
                    &mut m.items,
                    m.lifetime != Some(Lifetime::Automatic),
                    &m.name.name,
                );
            }
            SourceDefinition::Program(m) => {
                let m = Rc::make_mut(m);
                items(
                    &mut m.items,
                    m.lifetime != Some(Lifetime::Automatic),
                    &m.name.name,
                );
            }
            SourceDefinition::Package(p) => {
                let p = Rc::make_mut(p);
                let default_static = p.lifetime != Some(Lifetime::Automatic);
                for item in &mut p.items {
                    match item {
                        PackageItem::Function(f) if is_static(f.lifetime, default_static) => {
                            statements(&mut f.items, &HashSet::default(), &p.name.name);
                        }
                        PackageItem::Task(t) if is_static(t.lifetime, default_static) => {
                            statements(&mut t.items, &HashSet::default(), &p.name.name);
                        }
                        PackageItem::Class(c) => class(c),
                        _ => {}
                    }
                }
            }
            SourceDefinition::Class(c) => class(Rc::make_mut(c)),
            _ => {}
        }
    }
}

fn is_static(lifetime: Option<Lifetime>, default_static: bool) -> bool {
    match lifetime {
        Some(Lifetime::Static) => true,
        Some(Lifetime::Automatic) => false,
        None => default_static,
    }
}

fn class(c: &mut ClassDeclaration) {
    for item in &mut c.items {
        match item {
            // A static method qualifier does not change the automatic
            // lifetime of its variables (§8.6); only a lifetime does.
            ClassItem::Method(m) => match &mut m.kind {
                ClassMethodKind::Function(f) if is_static(f.lifetime, false) => {
                    statements(&mut f.items, &HashSet::default(), &c.name.name);
                }
                ClassMethodKind::Task(t) if is_static(t.lifetime, false) => {
                    statements(&mut t.items, &HashSet::default(), &c.name.name);
                }
                _ => {}
            },
            ClassItem::Class(nested) => class(nested),
            _ => {}
        }
    }
}

fn items(body: &mut [ModuleItem], default_static: bool, owner: &str) {
    for item in body {
        match item {
            ModuleItem::FunctionDeclaration(f)
                if is_static(f.lifetime, default_static) && f.name.scopes.is_empty() =>
            {
                statements(&mut f.items, &HashSet::default(), owner);
            }
            ModuleItem::TaskDeclaration(t)
                if is_static(t.lifetime, default_static) && t.name.scopes.is_empty() =>
            {
                statements(&mut t.items, &HashSet::default(), owner);
            }
            ModuleItem::InitialConstruct(i) if default_static => {
                statement(&mut i.stmt, &mut HashSet::default(), owner);
            }
            ModuleItem::AlwaysConstruct(a) if default_static => {
                statement(&mut a.stmt, &mut HashSet::default(), owner);
            }
            ModuleItem::FinalConstruct(f) if default_static => {
                statement(&mut f.stmt, &mut HashSet::default(), owner);
            }
            ModuleItem::GenerateRegion(g) => items(&mut g.items, default_static, owner),
            ModuleItem::GenerateFor(g) => items(&mut g.items, default_static, owner),
            ModuleItem::GenerateIf(g) => {
                for (_, branch) in &mut g.branches {
                    items(branch, default_static, owner);
                }
            }
            ModuleItem::GenerateCase(g) => {
                for arm in &mut g.arms {
                    items(&mut arm.items, default_static, owner);
                }
            }
            ModuleItem::ClassDeclaration(c) => class(c),
            ModuleItem::NestedModule(m) => {
                items(
                    &mut m.items,
                    m.lifetime != Some(Lifetime::Automatic),
                    &m.name.name,
                );
            }
            _ => {}
        }
    }
}

fn statements(body: &mut [Statement], automatic: &HashSet<String>, owner: &str) {
    let mut automatic = automatic.clone();
    for s in body {
        statement(s, &mut automatic, owner);
    }
}

fn statement(s: &mut Statement, automatic: &mut HashSet<String>, owner: &str) {
    match &mut s.kind {
        StatementKind::VarDecl {
            lifetime,
            declarators,
            ..
        } => {
            if *lifetime == Some(Lifetime::Automatic) {
                automatic.extend(declarators.iter().map(|d| d.name.name.clone()));
            } else if lifetime.is_none() && declarators.iter().any(|d| d.init.is_some()) {
                // Keep the permissive behavior for an initializer reading an
                // automatic loop/block variable; a static initializer cannot
                // legally refer to that variable (§6.21).
                let captures_automatic =
                    declarators.iter().filter_map(|d| d.init.as_ref()).any(|e| {
                        let mut names = HashSet::default();
                        collect_expr_idents(e, &mut names);
                        names.iter().any(|name| automatic.contains(name))
                    });
                if !captures_automatic {
                    *lifetime = Some(Lifetime::Static);
                    for d in declarators.iter().filter(|d| d.init.is_some()) {
                        crate::log_eprintln(&format!(
                            "[xezim][warning] variable '{}' in '{}' is implicitly static (§6.21); \
                             accepted because --relax-implicit-static is in effect",
                            d.name.name, owner
                        ));
                    }
                }
            }
        }
        StatementKind::SeqBlock { stmts, .. } | StatementKind::ParBlock { stmts, .. } => {
            statements(stmts, automatic, owner);
        }
        StatementKind::For { init, body, .. } => {
            let mut loop_automatic = automatic.clone();
            for i in init {
                if let ForInit::VarDecl { name, .. } = i {
                    loop_automatic.insert(name.name.clone());
                }
            }
            statement(body, &mut loop_automatic, owner);
        }
        StatementKind::Foreach { vars, body, .. } => {
            let mut loop_automatic = automatic.clone();
            loop_automatic.extend(vars.iter().flatten().map(|v| v.name.clone()));
            statement(body, &mut loop_automatic, owner);
        }
        StatementKind::If {
            then_stmt,
            else_stmt,
            ..
        } => {
            statement(then_stmt, &mut automatic.clone(), owner);
            if let Some(e) = else_stmt {
                statement(e, &mut automatic.clone(), owner);
            }
        }
        StatementKind::Case { items, .. } => {
            for i in items {
                statement(&mut i.stmt, &mut automatic.clone(), owner);
            }
        }
        StatementKind::While { body, .. }
        | StatementKind::DoWhile { body, .. }
        | StatementKind::Repeat { body, .. }
        | StatementKind::Forever { body }
        | StatementKind::TimingControl { stmt: body, .. }
        | StatementKind::Wait { stmt: body, .. } => statement(body, automatic, owner),
        _ => {}
    }
}
