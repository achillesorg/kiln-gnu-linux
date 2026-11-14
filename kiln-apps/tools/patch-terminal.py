#!/usr/bin/env python3
"""Apply the Kiln Terminal patches to a pristine foot 1.21.0 tree (run in the tree root)."""
def sub(p, a, b, n=1):
    t = open(p).read()
    assert a in t, (p, a)
    open(p, "w").write(t.replace(a, b, n))
sub("config.h", "    } colors;\n", "        bool kiln_user_set;   /* Kiln: user wrote a [colors] section: keep built-in scheme out */\n    } colors;\n")
sub("config.c", "parse_section_colors(struct context *ctx)\n{\n    struct config *conf = ctx->conf;\n",
    "parse_section_colors(struct context *ctx)\n{\n    struct config *conf = ctx->conf;\n    conf->colors.kiln_user_set = true;\n")
sub("config.c", "    conf->colors.use_custom.selection =\n        conf->colors.selection_fg >> 24 == 0 &&",
    "    if (!conf->colors.kiln_user_set)\n        kiln_colours_apply(conf, kiln_detect_dark());\n\n    conf->colors.use_custom.selection =\n        conf->colors.selection_fg >> 24 == 0 &&")
sub("config.c", '#include "config.h"\n', '#include "config.h"\n#include "kiln-colours.h"\n')
sub("config.c", '"/foot/foot.ini"', '"/kiln-terminal/kiln-terminal.ini"', 2)
sub("config.c", '"/.config/foot/foot.ini"', '"/.config/kiln-terminal/kiln-terminal.ini"')
sub("config.c", '.title = xstrdup("foot"),', '.title = xstrdup("Kiln Terminal"),')
sub("config.c", '(as_server ? xstrdup("footclient") : xstrdup("foot"))', '(as_server ? xstrdup("org.kiln.Terminal.client") : xstrdup("org.kiln.Terminal"))')
sub("terminal.h", "void term_damage_view(struct terminal *term);",
    "void term_damage_view(struct terminal *term);\nvoid term_apply_conf_colors(struct terminal *term);  /* Kiln: re-read colours from conf (scheme switch) */")
open("terminal.c", "a").write("""
void
term_apply_conf_colors(struct terminal *term)
{
    term->colors.fg = term->conf->colors.fg;
    term->colors.bg = term->conf->colors.bg;
    term->colors.cursor_fg = term->conf->cursor.color.text;
    term->colors.cursor_bg = term->conf->cursor.color.cursor;
    term->colors.selection_fg = term->conf->colors.selection_fg;
    term->colors.selection_bg = term->conf->colors.selection_bg;
    term->colors.use_custom_selection = term->conf->colors.use_custom.selection;
    memcpy(term->colors.table, term->conf->colors.table, sizeof(term->colors.table));
    term_damage_view(term);
    render_refresh(term);
}
""")
sub("main.c", "static void\nprint_usage(", """struct kiln_scheme_ctx { struct config *conf; struct wayland *wayl; };

static bool
fdm_sigscheme(struct fdm *fdm, int signo, void *data)
{
    struct kiln_scheme_ctx *c = data;
    kiln_colours_apply(c->conf, signo == SIGUSR1);   /* SIGUSR1 = dark, SIGUSR2 = light */
    tll_foreach(c->wayl->terms, it)
        term_apply_conf_colors(it->item);
    return true;
}

static void
print_usage(""")
sub("main.c", "    struct sigaction sig_ign = {.sa_handler = SIG_IGN};", """    struct kiln_scheme_ctx kiln_ctx = {&conf, wayl};
    if (!conf.colors.kiln_user_set) {
        fdm_signal_add(fdm, SIGUSR1, &fdm_sigscheme, &kiln_ctx);
        fdm_signal_add(fdm, SIGUSR2, &fdm_sigscheme, &kiln_ctx);
    }

    struct sigaction sig_ign = {.sa_handler = SIG_IGN};""")
sub("main.c", "    fdm_signal_del(fdm, SIGTERM);\n    fdm_signal_del(fdm, SIGINT);", "    fdm_signal_del(fdm, SIGUSR1);\n    fdm_signal_del(fdm, SIGUSR2);\n    fdm_signal_del(fdm, SIGTERM);\n    fdm_signal_del(fdm, SIGINT);")
sub("main.c", '#include "config.h"\n', '#include "config.h"\n#include "kiln-colours.h"\n')
