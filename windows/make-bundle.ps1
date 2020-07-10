mkdir boop-gtk.windows

# Binary
robocopy ..\target\release boop-gtk.windows boop-gtk.exe

# DLL's
$dlls = @(
    "atk-1.0-0.dll",          "gdk_pixbuf-2.0-0.dll",   "gdk-3-vs16.dll",      
    "libpng16.dll",           "cairo.dll",              "gio-2.0-0.dll"
    "libxml2.dll",            "cairo-gobject.dll",      "glib-2.0-0.dll",
    "pango-1.0-0.dll",        "epoxy-0.dll",            "gmodule-2.0-0.dll",
    "pangocairo-1.0-0.dll",   "ffi-7.dll",              "gobject-2.0-0.dll",
    "pangoft2-1.0-0.dll",     "fontconfig.dll",         "gtk-3-vs16.dll",
    "pangowin32-1.0-0.dll",   "freetype.dll",           "gtksourceview-3.0.dll",
    "fribidi-0.dll",          "iconv.dll",              "gdbus.exe",
    "intl.dll"
)
robocopy C:\gtk-build\gtk\x64\release\bin boop-gtk.windows $dlls

# Lib
robocopy C:\gtk-build\gtk\x64\release\lib\gdk-pixbuf-2.0 boop-gtk.windows\lib\gdk-pixbuf-2.0 /E

# Share
robocopy C:\gtk-build\gtk\x64\release\share\gtksourceview-3.0 boop-gtk.windows\share\gtksourceview-3.0 /E
robocopy C:\msys64\mingw64\share\icons boop-gtk.windows\share\icons /E

# Create archive
Compress-Archive -Path boop-gtk.windows\* -DestinationPath boop-gtk.windows.zip