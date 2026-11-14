/* Kiln Terminal: built-in Kiln flat-paper colour schemes (light and dark).
 * Added to foot by the Kiln GNU/Linux project; MIT licence like the rest of foot. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "config.h"
#include "kiln-colours.h"

struct scheme {
    uint32_t fg, bg, sel_fg, sel_bg, cursor_text, cursor_bg;
    uint32_t ansi[16];
};

static const struct scheme light = {
    .fg = 0x2B2926, .bg = 0xFAF9F5, .sel_fg = 0x2B2926, .sel_bg = 0xF2D5C6,
    .cursor_text = 0xFAF9F5, .cursor_bg = 0xD97757,
    .ansi = {0x2B2926, 0xB03A2E, 0x4F7A4F, 0x9A6F10, 0x3E6A94, 0x8A4F7D, 0x2F7F7A, 0xD9D2C3,
             0x6B665C, 0xC8473A, 0x5E8F5E, 0xB5841A, 0x4F82B2, 0xA3649A, 0x3F9690, 0xEFEBE0},
};

static const struct scheme dark = {
    .fg = 0xF3EFE6, .bg = 0x2B2926, .sel_fg = 0xF3EFE6, .sel_bg = 0x584035,
    .cursor_text = 0x2B2926, .cursor_bg = 0xD97757,
    .ansi = {0x3A3631, 0xE0705F, 0x8DB58A, 0xE0B04F, 0x7FA8CF, 0xC58BB9, 0x6FBDB6, 0xD9D2C3,
             0x6B665C, 0xEE8A7C, 0xA3C9A0, 0xEDC26B, 0x9BBFE0, 0xD6A3CC, 0x8ED0C9, 0xF3EFE6},
};

bool
kiln_detect_dark(void)
{
    const char *force = getenv("KILN_SCHEME");
    if (force != NULL && force[0] != '\0')
        return strcmp(force, "dark") == 0;

    const char *xdg = getenv("XDG_CONFIG_HOME");
    const char *home = getenv("HOME");
    char path[1024];
    if (xdg != NULL && xdg[0] != '\0')
        snprintf(path, sizeof(path), "%s/glaze/colour-scheme", xdg);
    else if (home != NULL)
        snprintf(path, sizeof(path), "%s/.config/glaze/colour-scheme", home);
    else
        return false;

    FILE *f = fopen(path, "r");
    if (f == NULL) {
        /* Pre-rename sessions wrote ~/.config/fede/colour-scheme. */
        const char *base = (xdg != NULL && xdg[0] != '\0') ? xdg : NULL;
        if (base != NULL)
            snprintf(path, sizeof(path), "%s/fede/colour-scheme", base);
        else
            snprintf(path, sizeof(path), "%s/.config/fede/colour-scheme", home);
        f = fopen(path, "r");
    }
    if (f == NULL)
        return false;
    char buf[16] = {0};
    if (fgets(buf, sizeof(buf), f) == NULL)
        buf[0] = '\0';
    fclose(f);
    return strncmp(buf, "dark", 4) == 0;
}

void
kiln_colours_apply(struct config *conf, bool is_dark)
{
    const struct scheme *s = is_dark ? &dark : &light;
    conf->colors.fg = s->fg;
    conf->colors.bg = s->bg;
    conf->colors.selection_fg = s->sel_fg;
    conf->colors.selection_bg = s->sel_bg;
    conf->cursor.color.text = s->cursor_text;
    conf->cursor.color.cursor = s->cursor_bg;
    /* top byte 0xFF means "use cursor colours as given"; foot sets bit 31 for explicit colours */
    conf->cursor.color.text |= 1u << 31;
    conf->cursor.color.cursor |= 1u << 31;
    for (int i = 0; i < 16; i++)
        conf->colors.table[i] = s->ansi[i];
    conf->colors.use_custom.selection = true;
}
