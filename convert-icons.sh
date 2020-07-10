mkdir ui/icons;
rm ui/icons/*.svg;
for f in submodules/Boop/Boop/Boop/Assets.xcassets/Icons/**/*.pdf; do 
echo "Processing $f file..";
name="$(basename -- ${f})";
name="${name:7}";
name="${name%.pdf}";
name="$name.svg";
pdf2svg $f ui/icons/$name;
done