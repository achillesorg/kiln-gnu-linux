/* Kiln GNU/Linux theme loader for GTK3 apps (Kiln Files).
 * Loads kiln-light.css / kiln-dark.css from KILN_THEME_DIR (compile-time) and follows
 * org.gnome.desktop.interface color-scheme (what glaze-colour-scheme sets) live.
 * Licence: GPL-2.0-or-later, same as the host application. */
#include <gtk/gtk.h>
#include <string.h>

static GtkCssProvider *kiln_provider = NULL;
static GSettings *kiln_gsettings = NULL;

static gboolean
kiln_theme_is_dark (void)
{
  gboolean dark = FALSE;
  const gchar *force = g_getenv ("KILN_SCHEME");
  if (force != NULL) return g_strcmp0 (force, "dark") == 0;
  if (kiln_gsettings != NULL)
    {
      gchar *s = g_settings_get_string (kiln_gsettings, "color-scheme");
      dark = g_strcmp0 (s, "prefer-dark") == 0;
      g_free (s);
    }
  return dark;
}

static void
kiln_theme_apply (void)
{
  const gchar *dir = g_getenv ("KILN_THEME_DIR");
  gchar *path;
  gboolean dark = kiln_theme_is_dark ();
  GtkSettings *settings = gtk_settings_get_default ();

  if (dir == NULL) dir = KILN_THEME_DIR;
  path = g_build_filename (dir, dark ? "kiln-dark.css" : "kiln-light.css", NULL);
  g_object_set (settings, "gtk-application-prefer-dark-theme", dark, NULL);
  if (!gtk_css_provider_load_from_path (kiln_provider, path, NULL))
    g_warning ("Kiln theme: cannot load %s", path);
  g_free (path);
}

static void
kiln_theme_changed (GSettings *s, const gchar *key, gpointer data)
{
  kiln_theme_apply ();
}

void
kiln_theme_init (void)
{
  GSettingsSchemaSource *src = g_settings_schema_source_get_default ();
  GtkSettings *settings = gtk_settings_get_default ();
  GdkScreen *screen = gdk_screen_get_default ();

  if (screen == NULL) return;
  g_object_set (settings, "gtk-icon-theme-name", "Kiln", "gtk-font-name", "Inter 10", NULL);
  if (src != NULL && g_settings_schema_source_lookup (src, "org.gnome.desktop.interface", TRUE) != NULL)
    {
      kiln_gsettings = g_settings_new ("org.gnome.desktop.interface");
      g_signal_connect (kiln_gsettings, "changed::color-scheme", G_CALLBACK (kiln_theme_changed), NULL);
    }
  kiln_provider = gtk_css_provider_new ();
  gtk_style_context_add_provider_for_screen (screen, GTK_STYLE_PROVIDER (kiln_provider), GTK_STYLE_PROVIDER_PRIORITY_USER);
  kiln_theme_apply ();
}
