leafpipe
========

A visualiser program that can control LED light sources, such as Nanoleaf Shapes
via the output of PipeWire (Linux audio stack).

The code to pull and analyse the audio files was heavily based upon the work in
https://github.com/BlankParenthesis/visualiser.

[See the blog post!](https://half-shot.uk/blog/leafpipe/)


## Getting Started

Copy the `config.sample.toml` file to `/home/username/.config/leafpipe/config.toml`.
You can also place the config in the same working directory you are running the application
from.

You will need to get an access token for your Nanoleaf Shapes device for
this to work. To do this:

```sh
# 1. Hold the power button for 5-7 seconds on your Nanoleaf device.
# 2. Run this (where IP is the IP address of your nanoleaf)
curl -X POST 'http://IP:16021/api/v1/new'
# 3. Save the output as nanoleaf_token in your config file.
```

You should now be able to run this app.

Remember to ensure you specify the correct recording source for this to work
in PipeWire. For music, you typically want to configure it to listen on a
"Monitor of SpeakerName" source.

