# Local GPUI compatibility patch

Source: crates.io `gpui-router` 0.2.7, published source checksum
`acbaabcb35218a1ea59a1e8e83b54609dd27c7a40b5dc691abf21f3d8ebf838a`.
Upstream: https://github.com/justjavac/gpui-router (MIT).

`src/route.rs` qualifies one call as `RenderOnce::render(...)`. GPUI Fast
also provides a blanket `View::render` implementation, so the original
method call is ambiguous. No routing behavior is changed. Remove this
patch when an upstream router release supports that GPUI API.

The root workspace patches the router's GPUI dependency to the same GPUI
Fast source used by the application and Kit.
