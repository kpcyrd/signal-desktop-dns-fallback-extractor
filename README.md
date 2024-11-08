# signal-desktop dns-fallback.json extractor

> [!NOTE]  
> The [dns-fallback.json](https://github.com/signalapp/Signal-Desktop/blob/main/build/dns-fallback.json) file is now tracked in signal-desktop git so this project is not needed anymore and has been retired.

This is some glue project to extract `./build/dns-fallback.json` from the official signal-desktop .deb and automatically makes it available as release artifact that can be used as a build input for [reproducible builds](https://reproducible-builds.org/).

For context see <https://github.com/signalapp/Signal-Desktop/issues/6823>

## License

The code in this repository is licensed `GPL-3.0-or-later`.
