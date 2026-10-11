// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The hand-written maintenance SQL's parameter/projection types (see the four repositories, all
// declared `user_owned` in metaphor.codegen.yaml).
pub use maintenance_schedule_repository::NewScheduleRow;
pub use maintenance_visit_part_repository::VisitPartRow;
pub use maintenance_visit_repository::{NewVisitRow, VisitCompletion, VisitCompletionRow};
pub use maintenance_request_repository::{NewRequestRow, RequestFieldUpdates, RequestTransitionRow, StageRefRow};
