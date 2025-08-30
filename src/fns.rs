use core::fmt::Debug;

#[derive(Debug, Copy, Clone, Default)]
pub struct MapOkFn<F>(F);

pub(crate) fn map_ok_fn<F>(f: F) -> MapOkFn<F> {
    MapOkFn(f)
}

#[derive(Debug, Copy, Clone, Default)]
pub struct MapErrFn<F>(F);

pub(crate) fn map_err_fn<F>(f: F) -> MapErrFn<F> {
    MapErrFn(f)
}

#[derive(Debug, Copy, Clone)]
pub struct InspectOkFn<F>(F);

pub(crate) fn inspect_ok_fn<F>(f: F) -> InspectOkFn<F> {
    InspectOkFn(f)
}

#[derive(Debug, Copy, Clone)]
pub struct InspectErrFn<F>(F);

pub(crate) fn inspect_err_fn<F>(f: F) -> InspectErrFn<F> {
    InspectErrFn(f)
}
