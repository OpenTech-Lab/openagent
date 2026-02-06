//! Skills module - Higher-level composable agent capabilities
//!
//! Skills are higher-level abstractions that compose multiple tools
//! to accomplish complex tasks. Unlike individual tools (which do one thing),
//! skills orchestrate multi-step workflows.
//!
//! ## Examples
//!
//! - **install_package**: Detects OS/package manager, installs, verifies
//! - **deploy**: Builds project, copies files, restarts services
//! - **diagnose**: Checks logs, system state, suggests fixes
//!
//! ## Adding a New Skill
//!
//! 1. Create a new file in `src/skills/` (e.g., `my_skill.rs`)
//! 2. Implement the `Skill` trait
//! 3. Add `mod my_skill;` and `pub use` in this file
//! 4. Register it in the skill registry

mod traits;
mod install_package;

pub use traits::{Skill, SkillContext, SkillRegistry};
pub use install_package::InstallPackageSkill;
