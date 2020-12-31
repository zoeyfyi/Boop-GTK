# Creating APNG's

1. Use Peek, or some other (as close to lossless as possible) screen recorder, ideally in webp
2. Extract frames with ffmpeg
3. Optionally, round corners using [`round-corners.sh`](../scripts/round-corners.sh)
4. Use the following to get a 40fps 600px wide looping APNG: `ffmpeg -r 40 -i frames/%*.png -plays 0 -vf scale=600:-1 demo.apng`

## Result

![APNG example](demo.apng)