mkdir ui/icons;
rm ui/icons/*.svg;
for dir in submodules/Boop/Boop/Boop/Assets.xcassets/Icons/*/; do 
    # get pdf
    files=( $dir*.pdf );
    file="${files[0]}";
    # extract file name
    name="${dir#submodules/Boop/Boop/Boop/Assets.xcassets/Icons/icons8-}";
    name="${name%.imageset/}";
    echo "Processing icon $name";
    name="$name.svg";
    # convert file
    pdf2svg $file ui/icons/$name;
done