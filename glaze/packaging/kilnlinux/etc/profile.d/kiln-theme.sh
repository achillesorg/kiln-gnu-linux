# Kiln GNU/Linux: make Qt apps follow the GTK theme and the Glaze light/dark scheme (see glaze-colour-scheme).
export QT_QPA_PLATFORMTHEME=gtk3
if [ "$(cat "${XDG_CONFIG_HOME:-$HOME/.config}/glaze/colour-scheme" 2>/dev/null)" = dark ]; then
  export QT_STYLE_OVERRIDE=Adwaita-Dark
fi
