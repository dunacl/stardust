//! Desktop and web entry point. Everything is in the library, so that the
//! phone builds (PLAN.md M6) can start the same app from their own.

fn main() {
    sg_app::run();
}
