# Vendored JavaScript

The examples' pages load these libraries from `/vendor/`, which the example
helper serves from this folder (plan §5.3, D4). Each is copied unmodified
from its release, with its licence beside it, and isn't counted in a phase's
size.

## OpenSeadragon 6.1.1

- **What:** a viewer for zoomable images, made of tiles at many levels.
  `ex-live-mosaic` shows the live mosaic's tiles with it (P38).
- **Licence:** BSD-3-Clause, in [`openseadragon-6.1.1/LICENSE.txt`](openseadragon-6.1.1/LICENSE.txt).
- **Released:** 2026-09-09, more than two weeks before it was taken on 2026-11-15.
- **From:** the npm package `openseadragon@6.1.1`,
  `https://registry.npmjs.org/openseadragon/-/openseadragon-6.1.1.tgz`, whose
  integrity matched the registry's: `sha512-g4iKe2q8B642SaBT3+x4ZF3/8b1TxPPydyjmtrOLgDrlYWMKd9ZXM+1Cb/xa9DVySkvs/7l6HR1nHKfD5F3Tsw==`.
- **Files,** from the package's `build/openseadragon/` and its root:

  | File | SHA-256 |
  |---|---|
  | `openseadragon.min.js` | `14d587cd3ee7c4861f1a41077ce642029364a21029956ba635bd8fb1202cf9ad` |
  | `LICENSE.txt` | `17da96b26ac706d2cbd06d4555dedb43026782f4943faf0156fe7acf89454228` |

  The package's navigation buttons' images are left out: the pages turn the
  buttons off. Its source map is left out too, so a browser's developer tools
  report it missing.

## VTK.js 38.0.4

- **What:** Kitware's library for 3D graphics on WebGL. `ex-volume` renders a
  volume's bricks with it, as a maximum intensity projection or composited
  (P41), and P43 draws a surface with it.
- **Licence:** BSD-3-Clause, in [`vtk.js-38.0.4/LICENSE`](vtk.js-38.0.4/LICENSE),
  with its notice of copyright in [`vtk.js-38.0.4/Copyright.txt`](vtk.js-38.0.4/Copyright.txt).
- **Released:** 2026-10-28, more than two weeks before it was taken on 2026-11-15.
- **From:** the npm package `vtk.js@38.0.4`, the UMD build that a page loads
  with a script tag, `https://registry.npmjs.org/vtk.js/-/vtk.js-38.0.4.tgz`,
  whose integrity matched the registry's:
  `sha512-EkhQQGrCDRauqwL9OlG3SfBJWjpiTc8UqVnj1R9pJ/dDDMQgpa6aKjIow4obLvaUolgxdUHppEXffUiQIabUpg==`.
- **Files,** from the package's root:

  | File | SHA-256 |
  |---|---|
  | `vtk.js` | `614497a30f96d3bcc31b975567c686e0ac1ed974b0eba281eb8ef03830abccf9` |
  | `LICENSE` | `975132688e5330fc9bef98cf3260af11d74f7ef2ffe8309b5d10bd73e212a5fc` |
  | `Copyright.txt` | `a8efeb737fe2a8c7fd511d61a4303c9278a402266059e471d48b57a48d895b3e` |

  Its source map, `vtk.js.map`, is left out, so a browser's developer tools
  report it missing.
