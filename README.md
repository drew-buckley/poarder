# poarder
Fast and simple bulk downloader for podcasts written in Rust. 

## Usage

Using poarder is easy. Simply find the RSS feed of your target podcast and run:
```
poarder -r <rss_url>
```

`poarder` by default downloads with 4 parallel `tokio` tasks. The number of tasks can be tweaked with the `--task-count` argument.

Call `poarder --help` for full list of options.

## Remarks

At the moment, the filename is prefixed with a timestamp corresponding to the publish date as stated in the RSS feed. In the future, I plan to add options for how to custom format the file names.

Most of the testing I've done has been with Acast podcast feeds (both free and paywalled), so while I suspect this should work fine with other providers, there's still more unknowns there.
