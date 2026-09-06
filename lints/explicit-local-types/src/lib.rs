#![feature(rustc_private)]

extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use rustc_errors::Applicability;
use rustc_hir::{
    HirId, LetStmt, LocalSource, PatKind,
    intravisit::{InferKind, Visitor, VisitorExt},
};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

dylint_linting::declare_late_lint! {
    /// Requires explicit types on non-primitive local bindings with nameable types.
    pub EXPLICIT_LOCAL_TYPES,
    Deny,
    "non-primitive local bindings must have explicit types"
}

fn exempt(ty: Ty<'_>) -> bool {
    matches!(
        ty.kind(),
        ty::Bool | ty::Char | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Never
    ) || ty.is_unit()
        || matches!(ty.kind(), ty::Ref(_, inner, _) if inner.is_str())
        || ty.walk().any(|arg| {
            arg.as_type().is_some_and(|inner| {
                matches!(
                    inner.kind(),
                    ty::Closure(..) | ty::Coroutine(..) | ty::CoroutineClosure(..) | ty::FnDef(..)
                ) || matches!(inner.kind(), ty::Alias(alias) if alias.is_opaque())
            })
        })
}

#[derive(Default)]
struct Placeholders(bool);

impl<'v> Visitor<'v> for Placeholders {
    fn visit_infer(&mut self, _: HirId, _: Span, _: InferKind<'v>) {
        self.0 = true;
    }
}

fn complete(annotation: &rustc_hir::Ty<'_>) -> bool {
    let mut placeholders: Placeholders = Placeholders::default();
    placeholders.visit_ty_unambig(annotation);
    !placeholders.0
}

impl<'tcx> LateLintPass<'tcx> for ExplicitLocalTypes {
    fn check_local(&mut self, cx: &LateContext<'tcx>, local: &'tcx LetStmt<'tcx>) {
        if !matches!(local.source, LocalSource::Normal)
            || local.span.from_expansion()
            || !cx
                .sess()
                .source_map()
                .span_to_snippet(local.span)
                .is_ok_and(|text| text.trim_start().starts_with("let"))
            || local.ty.is_some_and(complete)
            || matches!(local.pat.kind, PatKind::Wild)
        {
            return;
        }
        let ty: Ty<'tcx> = cx.typeck_results().pat_ty(local.pat);
        if exempt(ty) {
            return;
        }
        cx.opt_span_lint(
            EXPLICIT_LOCAL_TYPES,
            Some(local.pat.span),
            rustc_errors::DiagDecorator(|diag: &mut rustc_errors::Diag<'_, ()>| {
                diag.primary_message("non-primitive local binding needs an explicit type");
                diag.span_suggestion(
                    local.ty.map_or(local.pat.span.shrink_to_hi(), |ty| ty.span),
                    "add a type annotation",
                    local
                        .ty
                        .map_or_else(|| format!(": {ty}"), |_| ty.to_string()),
                    Applicability::MaybeIncorrect,
                );
            }),
        );
    }
}

#[test]
fn ui() {
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui")
        .rustc_flags(["--edition=2024"])
        .run();
}
