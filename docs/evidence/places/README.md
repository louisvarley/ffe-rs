# The navigation computer's places list

`original-gateway.png` — the executed original at Gateway, cropped from Figure
20 ("The Navigation Computer at Gateway") of the *Frontier: First Encounters*
manual (`ffemanual.pdf`, p. 21). The 12 places are nine down the left column and
three down the right: Fortress Saskatoon, Old Curie, Old Blackelk, Turner,
Turner Town, Dublin Citadel, Sarajevo, Seattle, Larson | New Chernobyl,
Sarajevo, Micca Town. The markers' visible grey squares start at x 12 and
x 232; the names at x 25 and x 245; the rows are 15 logical pixels apart from
y 5. There is no panel or background, and the Set/Act HUD, the Alt/Hope lines
and the console stay visible.

`before.png` — the port before this change: the same 12 names and the same
landing view, but the marker top-left is at x 5 (visible square x 7) and the
names at x 20, five pixels left of F1099; the first column sat against the
screen edge.

`after.png` — the port with F1099's geometry (x 10 / 230, names x+15, nine
rows), the list drawn after F1099's HUD and F272's message window. Nothing in
the console covers the list: its last row ends at y 133 and F272's window starts
at y 158.

`before-and-after.png` — the two port frames stacked for the 5-pixel column
shift.

`after-sol.png` — the same code with Sol's real 18 places (nine and nine), the
original's order and names.

Both port frames are written by the ignored test
`crates/ffe-rs/tests/console_view.rs::the_gateway_places_frame_over_a_landing_view`
(and `the_sol_places_list_frame_has_the_originals_names`) with
`FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir>`; the 12 Gateway names are
injected because the port's new game still starts at Ross 154 (see
`docs/notes/labels.md`, "Starting system").
