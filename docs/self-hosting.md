# Hosting json-diff in your organisation

json-diff's web app can run on your own infrastructure instead of
[json-diffrs.site](https://json-diffrs.site): for example on an intranet, behind
your SSO, or in your own cloud account. This guide is for the team that runs it
and the team that reviews it.

## What you are hosting

- **Static files only.** Each [release](https://github.com/ultra-infinity/json-diffrs/releases/latest)
  includes `json-diff-web-<version>.zip`: one folder holding an `index.html`, an
  `assets/` folder (JavaScript, styles, fonts and json-diff's engine compiled to
  WebAssembly), `LICENSE.txt` (MIT) and `THIRD_PARTY_NOTICES.txt`. There is no
  server-side code, database or service to run.
- **All processing happens in the user's browser.** Files and folders that users
  open are read and compared inside the page, and PDFs are made there. Nothing is
  uploaded to your server or to anyone else's.
- **No outbound connections.** The page carries a Content Security Policy that
  lets it load files from, and connect to, its own origin only. The browser refuses
  any other request. There is no telemetry, analytics, cookies or account system.
  Settings stay in each user's browser storage.
- **Open source.** The code is at <https://github.com/ultra-infinity/json-diffrs>,
  under the MIT license, and the zip is built from the release's tagged commit
  (`nix build .#web-zip`).

## Checking the download

Download the zip and `SHA256SUMS` from the same release, then:

```sh
sha256sum --check --ignore-missing SHA256SUMS      # Linux, macOS (shasum -a 256 -c)
```

On Windows, compare `Get-FileHash json-diff-web-<version>.zip` with the zip's line
in `SHA256SUMS`.

## Requirements for any web server

1. **Serve the folder's contents as static files.** They work at a site's root or
   under any path, such as `https://intranet.example.com/tools/json-diff/`.
2. **Send `.wasm` files as `application/wasm`.** Browsers refuse to run json-diff's
   engine otherwise, and the page stays empty.
3. **Use HTTPS.** Chrome and Edge only offer their "view files" folder picker on
   secure pages; without it users get the older picker, worded as "upload".
4. **Caching (recommended):** `index.html` with `Cache-Control: no-cache`, so a new
   version shows up at once. Files in `assets/` have content hashes in their names
   and can be cached for a year (`public, max-age=31536000, immutable`).
5. **Optional:** send the Content Security Policy as an HTTP header too. The page
   already enforces it through a `<meta>` tag in `index.html`. The policy text is in
   [`ui/src/csp.ts`](../ui/src/csp.ts) and is shown in the app under **Help → Privacy &
   Security**.

## AWS: S3 and CloudFront

S3 stores the files; CloudFront adds HTTPS, your domain and caching.

**1. Unpack the release.**

```sh
unzip json-diff-web-<version>.zip
cd json-diff-web-<version>
```

**2. Create a private bucket.** Keep *Block all public access* on: only CloudFront
will read it.

```sh
aws s3 mb s3://example-json-diff --region eu-west-1
```

**3. Upload the files,** with `index.html` and the other top-level files never
cached and `assets/` cached for a year:

```sh
aws s3 sync . s3://example-json-diff/ --delete --exclude "assets/*" \
  --cache-control "no-cache"
aws s3 sync assets s3://example-json-diff/assets/ --delete \
  --cache-control "public, max-age=31536000, immutable"
```

Check the engine's content type:

```sh
aws s3api head-object --bucket example-json-diff \
  --key "assets/$(ls assets | grep '\.wasm$')" --query ContentType
```

It must print `"application/wasm"`. If not, set it:

```sh
aws s3 cp s3://example-json-diff/assets/ s3://example-json-diff/assets/ --recursive \
  --exclude "*" --include "*.wasm" --content-type application/wasm \
  --cache-control "public, max-age=31536000, immutable" --metadata-directive REPLACE
```

**4. Create a CloudFront distribution:**

- **Origin:** the bucket, with *Origin access control*. Let CloudFront update the
  bucket policy.
- **Default root object:** `index.html`.
- **Viewer protocol policy:** *Redirect HTTP to HTTPS*.
- **Your domain:** add it under *Alternate domain names*, with a certificate from
  **ACM in us-east-1** (CloudFront only uses certificates from that region), and
  point a DNS `CNAME` or Route 53 alias at the distribution.
- **Response headers policy (optional):** *Strict-Transport-Security*, and a
  *Content-Security-Policy* with the policy from `ui/src/csp.ts`.

**5. Restrict access** if json-diff should not be public:

- **By network:** attach AWS WAF to the distribution with an IP allow-list for your
  offices and VPN.
- **By user:** put it behind your identity provider, for example with an
  authenticating proxy or Lambda@Edge in front of CloudFront.

**Without the command line:** AWS Amplify Hosting can *Deploy without Git* from an
uploaded zip. It expects `index.html` at the top of the zip, so re-zip the folder's
contents first (`cd json-diff-web-<version> && zip -r ../site.zip .`).

## Other web servers

**nginx.** Recent versions list `application/wasm wasm;` in `mime.types`; add it there
if it is missing.

```nginx
server {
    listen 443 ssl;
    server_name json-diff.example.com;
    root /srv/json-diff;                 # the folder's contents

    location = /index.html { add_header Cache-Control "no-cache"; }
    location /assets/ {
        add_header Cache-Control "public, max-age=31536000, immutable";
    }
}
```

**Apache** (in the virtual host, or a `.htaccess` file in the folder):

```apache
AddType application/wasm .wasm
<Files "index.html">
    Header set Cache-Control "no-cache"
</Files>
```

**IIS** (a `web.config` file in the folder):

```xml
<configuration>
  <system.webServer>
    <staticContent>
      <remove fileExtension=".wasm" />
      <mimeMap fileExtension=".wasm" mimeType="application/wasm" />
    </staticContent>
  </system.webServer>
</configuration>
```

**Azure Blob Storage static websites, Google Cloud Storage and similar** work the same
way as S3: upload the folder's contents, check the `.wasm` content type, and put a CDN
or load balancer with HTTPS in front.

## Checking the deployment

1. Open the address. A sample comparison should appear within a few seconds. If the
   page stays empty, the `.wasm` content type is the usual cause.
2. In the browser's developer tools (Network tab), every request should go to your
   own address.
3. Open a folder or two files to compare. You should see json-diff's note that files
   stay on the computer, then the browser's own prompt.

## Updating

Each release has a new zip. Replace the folder's contents with the new version's
(or repeat the upload) and clear any cache of `index.html`. For CloudFront:

```sh
aws cloudfront create-invalidation --distribution-id <ID> --paths "/index.html"
```

The files in `assets/` have new names in each release, so they need no invalidation.
Release notes are in [CHANGELOG.md](../CHANGELOG.md), and inside the app under
**Help → What's New**.

## Building it yourself

To build the same folder from source, see
[Development](../README.md#development) in the README. It can be built with or
without Nix.
