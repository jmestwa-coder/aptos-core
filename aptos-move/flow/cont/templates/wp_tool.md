{# Shared WP tool reference and inference guidelines #}
{% if once(name="wp_tool") %}

### WP Tool

Use `{{ tool(name="move_package_spec_infer") }}`, a weakest precondition (WP)
inference tool for deriving specs. Do not run this tool outside of this workflow.

Parameters:

- **`package_path`** (required) — path to the Move package directory.
- **`filter`** (optional) — `module_name` or `module_name::function_name`.
  When omitted, all target modules are inferred.
- **`spec_output`** (optional, default `"inline"`) — where to write inferred specs.
  `"inline"` injects specs into the original source files.
  `"file"` writes separate `.spec.move` files alongside the sources, leaving
  originals untouched. Use `"file"` when the user asks for specs in a separate file.

If the program contains loops, they are broken into exit and iteration points.
Loop variables are havoced and the loop invariant is expected to fix them.
Without loop invariants, derived WPs leave values in arbitrary state, resulting
in `[inferred = vacuous]` properties. You must add loop invariants to fix this.

**Before every WP re-run:** remove all existing `[inferred]`,
`[inferred = vacuous]`, and `[inferred = sathard]`
conditions from spec blocks in scope. The WP tool will regenerate them; keeping
stale copies leads to duplicate conditions.

{% endif %}
