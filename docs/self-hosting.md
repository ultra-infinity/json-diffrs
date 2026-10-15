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

## AWS in the web console

For teams that use the AWS Management Console rather than the command line. Two
routes:

- **Amplify Hosting:** the simplest. Upload one zip; AWS handles HTTPS, caching and
  the address.
- **S3 and CloudFront:** a few more steps, but more control: a private bucket,
  restricting access by IP with AWS WAF, and security headers.

AWS renames console buttons from time to time; if a label below differs slightly,
look for the nearest equivalent.

### Prepare the files

1. Download `json-diff-web-<version>.zip` from the release and check it (see
   [Checking the download](#checking-the-download)).
2. Unzip it. You get a folder `json-diff-web-<version>` containing `index.html`,
   `LICENSE.txt`, `THIRD_PARTY_NOTICES.txt` and an `assets` folder.

### Route 1: Amplify Hosting

1. **Make a zip with `index.html` at the top.** Amplify needs the files themselves in
   the zip, not the folder around them. Open the `json-diff-web-<version>` folder,
   select everything inside it, and compress the selection: on Windows, right-click →
   *Send to* → *Compressed (zipped) folder*; on macOS, right-click → *Compress*.
2. In the console, open **AWS Amplify** → **Create new app** → **Deploy without Git**
   → **Next**.
3. Enter an app name (for example `json-diff`) and a branch or environment name (for
   example `production`), then drag your zip into the upload area and choose **Save and
   deploy**.
4. When the deployment shows as complete, open the domain Amplify gives you (ending in
   `.amplifyapp.com`). The json-diff sample should appear. If the page stays empty, see
   [Checking the deployment](#checking-the-deployment).
5. **Your own domain (optional):** in the app, open **Hosting** → **Custom domains** →
   **Add domain** and follow the steps. Amplify issues the certificate itself.
6. **Restricting access (optional):** **Hosting** → **Access control** can require a
   username and password for the whole site. For IP or single-sign-on restrictions,
   use route 2.

**Updating:** open the app, choose the deployment, and upload the next release's zip,
prepared the same way.

### Route 2: S3 and CloudFront

**1. Create a private bucket.**

1. Open **S3** → **Create bucket**.
2. Enter a name (for example `example-json-diff`) and choose your region.
3. Leave **Block all public access** ticked: only CloudFront will read the bucket.
4. Choose **Create bucket**.

**2. Upload the files.**

1. Open the bucket → **Upload**.
2. Drag in the *contents* of the `json-diff-web-<version>` folder (`index.html`,
   `LICENSE.txt`, `THIRD_PARTY_NOTICES.txt` and the `assets` folder), not the folder
   itself, so `index.html` ends up at the top of the bucket.
3. Choose **Upload**, then **Close** when it finishes.

**3. Check the engine's content type.** Browsers only run json-diff's engine if it is
labelled as WebAssembly.

1. In the bucket, open `assets/` and click the file ending in `.wasm`.
2. Under **Metadata**, the `Content-Type` must be `application/wasm`.
3. If it is anything else: select the file → **Actions** → **Edit metadata** → change
   (or add) **System defined** `Content-Type` to `application/wasm` → **Save changes**.

**4. Create a CloudFront distribution.**

1. Open **CloudFront** → **Create distribution**.
2. **Origin domain:** pick your bucket from the list (the entry ending in
   `.s3.<region>.amazonaws.com`, not one with `s3-website` in it).
3. **Origin access:** choose **Origin access control settings (recommended)** →
   **Create new OAC** → **Create**.
4. **Viewer protocol policy:** **Redirect HTTP to HTTPS**.
5. **Web Application Firewall:** choose whether to enable security protections now;
   you can add an IP allow-list later (step 7).
6. **Default root object:** `index.html`.
7. Choose **Create distribution**.
8. A banner says the S3 bucket policy must be updated. Choose **Copy policy**, open the
   bucket → **Permissions** → **Bucket policy** → **Edit**, paste it, and **Save
   changes**. Without this, CloudFront cannot read the files.
9. Wait until the distribution's **Last modified** shows a date instead of
   *Deploying* (a few minutes), then open its **Distribution domain name** (ending in
   `.cloudfront.net`). The json-diff sample should appear.

**5. Your own domain (optional).**

1. Open **Certificate Manager** and switch the region (top right) to **US East (N.
   Virginia) us-east-1**: CloudFront only uses certificates from there.
2. **Request** → **Request a public certificate** → enter your domain (for example
   `json-diff.example.com`) → **DNS validation** → **Request**.
3. Open the certificate and add the validation `CNAME` record it shows to your DNS (if
   your domain is in Route 53, **Create records in Route 53** does it). Wait until the
   status is **Issued**.
4. Back in CloudFront, open the distribution → **General** → **Edit**: add the domain
   under **Alternate domain name**, choose the certificate under **Custom SSL
   certificate**, and **Save changes**.
5. In your DNS, point the domain at the distribution's domain name: a `CNAME` record,
   or in Route 53 an **Alias** record to the CloudFront distribution.

**6. Security headers (optional).** The page already enforces its Content Security
Policy itself; these add it, and HTTPS-only browsing, as HTTP headers too.

1. **CloudFront** → **Policies** → **Response headers** → **Create response headers
   policy**.
2. Under **Security headers**, turn on **Strict Transport Security**, and **Content
   Security Policy** with the policy from [`ui/src/csp.ts`](../ui/src/csp.ts) (also shown
   in the app under **Help → Privacy & Security**). **Create**.
3. Open the distribution → **Behaviors** → select the default behavior → **Edit** →
   **Response headers policy** → choose your policy → **Save changes**.

**7. Restrict access by network (optional).**

1. Open **WAF & Shield** → **IP sets** → **Create IP set**, with the region set to
   **Global (CloudFront)**, listing your office and VPN address ranges.
2. **Web ACLs** → **Create web ACL**, resource type **Amazon CloudFront
   distributions**, associated with your distribution. Add a rule that **allows**
   requests from the IP set, and set the **default action** to **Block**.

**Updating to a new release:**

1. Upload the new release's folder contents to the bucket as in step 2. Files with the
   same names are replaced; the old `assets` files can be deleted or left, as nothing
   refers to them any more.
2. Check the `.wasm` content type again (step 3).
3. In CloudFront, open the distribution → **Invalidations** → **Create invalidation**
   → enter `/*` → **Create invalidation**, so visitors get the new version straight
   away.

## AWS with the command line

The same S3 and CloudFront setup as route 2 above, scripted with the AWS CLI. S3
stores the files; CloudFront adds HTTPS, your domain and caching.

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
