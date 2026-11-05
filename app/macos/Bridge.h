// What the app exports to the native interface (app/src/native.rs).

#include <stdbool.h>

/// The file behind a url of the app's UI, or null; freed with arbalete_free_string.
char *arbalete_ui_file(const char *url);

void arbalete_free_string(char *string);

/// The window `id` resized, went into full screen or out of it.
void arbalete_window_resized(unsigned int id);

/// The window `id` came in front, or left it.
void arbalete_window_activated(unsigned int id, bool active);

/// Whether the window `id` may close now: the app may first close its pages.
bool arbalete_window_should_close(unsigned int id);

/// The window `id` closed.
void arbalete_window_closed(unsigned int id);

/// Whether the Little Arbalete window `id` may close now: the app first closes its page, which
/// closes the window.
bool arbalete_little_should_close(unsigned int id);
