param (
    [bool]$archive = $true,
    [string]$build = "release"
)

mkdir boop-gtk.windows

# Binary
robocopy ..\target\$build boop-gtk.windows boop-gtk.exe

# DLL's
robocopy C:\gtk-build\gtk\x64\release\bin boop-gtk.windows *.dll

# Lib
robocopy C:\gtk-build\gtk\x64\release\lib\gdk-pixbuf-2.0 boop-gtk.windows\lib\gdk-pixbuf-2.0 /E

# Share
robocopy C:\gtk-build\gtk\x64\release\share\gtksourceview-3.0 boop-gtk.windows\share\gtksourceview-3.0 /E
robocopy C:\msys64\mingw64\share\icons boop-gtk.windows\share\icons /E

# Create archive
if ($archive) {
    try {
        Compress-Archive -Force -Path boop-gtk.windows\* -DestinationPath boop-gtk.windows.zip
    } catch {
        "Failed to create archive"
        $_
    }
}

# supress robocopy non-zero success
if ($lastexitcode -lt 20) { $global:lastexitcode = 0 }