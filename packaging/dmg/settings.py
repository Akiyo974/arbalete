# The disk image's window, for dmgbuild (mise run release): the app and a link to Applications
# over the background drawn by background.swift, like the "Trajectory" design.
import os

app = defines["app"]  # noqa: F821, given by dmgbuild with -D app=...

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}

# From the repository's root, where mise runs it.
background = "packaging/dmg/background.tiff"
# The background's size, plus the title bar.
window_rect = ((200, 200), (660, 428))
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
default_view = "icon-view"
icon_size = 112
text_size = 13
# Under each end of the background's arc.
icon_locations = {os.path.basename(app): (165, 206), "Applications": (495, 206)}
