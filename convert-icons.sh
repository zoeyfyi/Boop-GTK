rm resources/icons/scalable/actions/*.svg;
for dir in submodules/Boop/Boop/Boop/Assets.xcassets/Icons/*/; do 
    # get pdf
    files=( $dir*.pdf );
    file="${files[0]}";
    # extract file name
    name="${dir#submodules/Boop/Boop/Boop/Assets.xcassets/Icons/icons8-}";
    name="${name%.imageset/}";
    name=${name,,}
    echo "Processing icon $name";
    name="boop-gtk-$name-symbolic.svg";
    # convert file
    pdf2svg $file resources/icons/scalable/actions/$name;
    # convert strokes
    # ./submodules/svg-stroke-to-path/svg-stroke-to-path SameStrokeColor 'stroke="#000"' resources/icons/scalable/actions/$name
done