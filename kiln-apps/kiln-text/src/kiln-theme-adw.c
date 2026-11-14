/* Kiln GNU/Linux theme loader for libadwaita (GTK4) apps.
 * Loads kiln-light.css / kiln-dark.css from KILN_THEME_DIR following AdwStyleManager's dark state
 * (which tracks org.gnome.desktop.interface color-scheme, i.e. what glaze-colour-scheme sets). */
#include <adwaita.h>

static GtkCssProvider *kiln_provider = NULL;

static void
kiln_theme_apply (void)
{
  const gchar *dir = g_getenv ("KILN_THEME_DIR");
  const gchar *force = g_getenv ("KILN_SCHEME");
  gboolean dark;
  gchar *path;

  if (dir == NULL) dir = KILN_THEME_DIR;
  if (force != NULL)
    dark = g_strcmp0 (force, "dark") == 0;
  else
    dark = adw_style_manager_get_dark (adw_style_manager_get_default ());
  path = g_build_filename (dir, dark ? "kiln-dark.css" : "kiln-light.css", NULL);
  gtk_css_provider_load_from_path (kiln_provider, path);
  g_free (path);
}

static void
kiln_theme_dark_changed (GObject *obj, GParamSpec *pspec, gpointer data)
{
  kiln_theme_apply ();
}

static gboolean
kiln_force_scheme (gpointer data)
{
  const gchar *force = g_getenv ("KILN_SCHEME");
  adw_style_manager_set_color_scheme (adw_style_manager_get_default (),
                                      g_strcmp0 (force, "dark") == 0 ? ADW_COLOR_SCHEME_FORCE_DARK : ADW_COLOR_SCHEME_FORCE_LIGHT);
  return G_SOURCE_REMOVE;   /* test aid only; apps that bind the scheme to their own setting (Kiln Text) ignore it */
}

void
kiln_theme_init (void)
{
  GdkDisplay *display = gdk_display_get_default ();
  const gchar *force = g_getenv ("KILN_SCHEME");

  if (display == NULL) return;
  if (force != NULL)
    g_timeout_add (500, kiln_force_scheme, NULL);   /* after the app applied its own setting */
  g_object_set (gtk_settings_get_for_display (display), "gtk-icon-theme-name", "Kiln", "gtk-font-name", "Inter 10", NULL);
  kiln_provider = gtk_css_provider_new ();
  gtk_style_context_add_provider_for_display (display, GTK_STYLE_PROVIDER (kiln_provider), GTK_STYLE_PROVIDER_PRIORITY_USER);
  g_signal_connect (adw_style_manager_get_default (), "notify::dark", G_CALLBACK (kiln_theme_dark_changed), NULL);
  kiln_theme_apply ();
}
