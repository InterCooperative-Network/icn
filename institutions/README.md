# Institution Packages

This directory holds institution-specific packages that are kept inside the ICN monorepo for now but should behave as if they were already externally owned repositories.

Rules:
- Generic institutional primitives stay in ICN core and `icn/apps/`.
- Institution vocabulary, seed data, templates, workflows, local views, and migration glue live here.
- Packages in this directory should avoid reaching back into core with institution-specific type pressure.

The first package scaffold is [`nycn/`](nycn/).
