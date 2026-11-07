# Mooring public website and search discovery

## 1. Goal and failure condition
Help a new visitor understand Mooring and reach the accurate installation path. Adopt a single static public page before adding a content platform. Failure: broken installation links, misleading download claims, inaccessible content without JavaScript, or a deployment that publishes unrelated repository documents. Search impressions and non-brand clicks are outcome measures, not guaranteed by launch.

## 2. Visitor path
Search/share → public product page → capabilities and limitations → source installation → README or issue. Product page and Pages deployment are new; source installation and feedback already exist. Payments, accounts and new analytics are not applicable.

## 3. Public contract
URL: https://majiayu000.github.io/mooring/. macOS source installation is the documented public path; no prebuilt download or cross-platform release is claimed. HTML contains the full content without JavaScript. Canonical, Open Graph, Twitter card, SoftwareSourceCode JSON-LD and sitemap describe the same page. Google URL-prefix ownership uses the account's existing verification token. A project-level robots.txt cannot control the github.io host, so none is fabricated.

## 4. Implementation
Static docs/site/index.html and sitemap.xml. Pages workflow stages only these files and the existing app icon. README links and repository homepage point visitors to the published site after deployment. Personal release-checklist files are excluded.

## 5. Acceptance
Passed locally: 1440px and 390px browser layouts with JavaScript disabled; installation anchor and native FAQ disclosure; no horizontal page overflow after constraining the installation grid; seven distinct external links returned HTTP 200; canonical, one H1, JSON-LD and sitemap parsed. Desktop and mobile screenshots visually inspected. Pending: workflow deployment and public readback. Native application behavior is unchanged; broad Rust gates remain enforced by existing repository CI.

## 6. Publication
User authorized implementation, testing, push and merge. Publish through a PR and existing required checks. Google property verification, sitemap submission and URL request are separate acceptance steps. No community posts are part of this change.

## 7. Baseline and follow-up
Before launch: no GitHub Pages site or product-site Search Console property verified in this task. GitHub traffic snapshot saved locally on 2026-11-08; it includes bots and internal views and is not a user count. Google search baseline is unknown, not zero. After launch: inspect sitemap fetch status on 2026-11-09; if still failing on 2026-11-15, diagnose the actual fetch error. Review impressions/clicks on 2026-11-22 and 2026-12-06. These dates are a follow-up plan, not an automatic scheduled job.

## 8. Decision
Adopt the static site if public and browser checks pass. Do not add a framework, duplicate documentation platform, or analytics account for this scope. Indexing and growth remain unverified until Google reports actual data. Update this record with acceptance and deployment evidence.
