#!/bin/bash
FILES=frames/*
for f in $FILES
do
  echo "Processing $f file..."
  convert $f \
    \( +clone  -alpha extract \
      -draw 'fill black polygon 0,0 0,15 15,0 fill white circle 15,15 15,0' \
      \( +clone -flip \) -compose Multiply -composite \
      \( +clone -flop \) -compose Multiply -composite \
    \) -alpha off -compose CopyOpacity -composite $f.rounded.png
  # cat $f
done
