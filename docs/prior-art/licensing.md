# Takyon licence research

Prepared 2026-10-23 for Nikhil Sah. Scope: choosing a licence for github.com/Sah-Nikhil/Takyon that keeps the source public, lets anyone use, study, modify and contribute, and stops anyone from selling, repackaging, rebranding or commercially exploiting any part of it.

> **Superseded in part by [ADR-0039](../adr/0039-source-available-under-cc-by-nc-sa.md).**
> This report recommends PolyForm Noncommercial 1.0.0. That recommendation was
> withdrawn: PolyForm NC's Distribution License ("The licensor grants you an
> additional copyright license to distribute copies of the software.") carries
> no "permitted purpose" limit, so anyone may plausibly distribute copies for a
> fee. Section 7.2 below flags this for institutions only, which understates it.
> Takyon adopted the runner-up, CC BY-NC-SA 4.0, whose every grant, sharing
> included, is limited to NonCommercial purposes. The rest of the survey, the
> contribution analysis, the trademark notes and the dependency obligations
> still hold. Where this report says "PolyForm NC", read the ADR.

> **Not legal advice.** This is research by an AI assistant, not a lawyer. Licence terms are quoted from their official texts as of the date above; interpretations are labelled **[inference]**. Before relying on any of this against a real infringer, or before registering a trademark, talk to an IP lawyer in your jurisdiction (India, judging by the project's timezone; confirm).

Labels used below: **[observed]** means read directly in the cited source. **[inference]** means my reading or a judgement call.

---

## 1. Short answer

1. **No OSI-approved open source licence can do what you want.** Banning commercial use is exactly what the Open Source Definition forbids. If Takyon carries a noncommercial clause, it is **source-available**, and it should never be described as "open source".
2. **Primary pick: PolyForm Noncommercial 1.0.0** (`PolyForm-Noncommercial-1.0.0`). It is the only professionally drafted, SPDX-listed software licence that bans *all* commercial use (not just competing or hosted use) while still granting modification, redistribution, and a patent licence.
3. **Runner-up: CC BY-NC-SA 4.0** (`CC-BY-NC-SA-4.0`). It is widely recognised and adds share-alike, but Creative Commons itself recommends against using it for software, and it has no patent grant.
4. Whatever licence you choose, three things sit outside it and you need to add them yourself: a **CLA** for contributions, a **trademark policy** for the name "Takyon", and **third-party notices** for your MIT/Apache dependencies.
5. One decision only you can make (section 7.3): under PolyForm Noncommercial, **using Takyon at a for-profit job is very likely not permitted.** For a launcher this matters. I recommend adding a narrow extra permission for that.

---

## 2. Can an "open source" licence satisfy this? No.

**Open Source Definition, clause 6** [observed], <https://opensource.org/osd>:

> "6. No Discrimination Against Fields of Endeavor. The license must not restrict anyone from making use of the program in a specific field of endeavor. For example, it may not restrict the program from being used in a business, or from being used for genetic research."

Clause 5 adds [observed]: "The license must not discriminate against any person or group of persons." A ban on commercial use restricts use "in a business", which is the clause's own example. So no licence with a noncommercial condition can be OSI-approved.

**FSF agrees from the free-software side** [observed], <https://www.gnu.org/philosophy/free-sw.html>, Freedom 0 is "The freedom to run the program as you wish, for any purpose", and under "Free software can be commercial": "'Free software' does not mean 'noncommercial.'" and "a free program must be available for commercial use, commercial development, and commercial distribution."

**Confirmation in SPDX metadata** [observed], SPDX License List 3.29.0 (2026-09-16), <https://spdx.org/licenses/>: `PolyForm-Noncommercial-1.0.0`, `PolyForm-Small-Business-1.0.0`, `CC-BY-NC-SA-4.0`, `BUSL-1.1`, `FSL-1.1-MIT`, `FSL-1.1-ALv2`, `Elastic-2.0` and `SSPL-1.0` all carry `isOsiApproved: false`. `AGPL-3.0-only`, `MPL-2.0`, `MIT` and `Apache-2.0` are `true`.

### What this means for naming

- **Say "source-available", never "open source".** OSI's position [observed], <https://opensource.org/blog/the-sspl-is-not-an-open-source-license>: companies may choose proprietary or source-available licences, but may not "claim or imply that software under a license that has not been approved by the Open Source Initiative" is open source, and doing so is "deception, plain and simple."
- PolyForm describes itself the same way [observed], <https://polyformproject.org/about>: open source and free software are not its category. It is an alternative for people who want to licence source code with limited rights.
- Commons Clause's own FAQ also says its software is not open source [observed], <https://commonsclause.com/>.
- Practical consequence [inference]: some communities, package registries, distro repositories (Debian, Fedora, winget's community expectations), "awesome open source" lists and OSS grant programmes exclude source-available projects. Expect occasional pushback on Hacker News or Reddit if the README is unclear. Precise wording avoids most of it.

---

## 3. Candidate survey

### 3.1 Summary table

Goal columns: **NC** = bans all commercial use; **Part** = covers copying part of the code into a commercial product; **Mod** = lets people modify and share changes; **Forever** = restriction never expires.

| Licence | SPDX id | NC | Part | Mod | Forever | Patent grant | Fit |
|---|---|---|---|---|---|---|---|
| PolyForm Noncommercial 1.0.0 | `PolyForm-Noncommercial-1.0.0` | Yes | Yes | Yes | Yes | Yes | **Best** |
| PolyForm Strict 1.0.0 | none in SPDX 3.29 | Yes | Yes | **No** | Yes | Yes | Fails "modify / contribute" |
| PolyForm Shield 1.0.0 | none in SPDX 3.29 | No (competing only) | Partly | Yes | Yes | Yes | Fails "no commercial at all" |
| PolyForm Small Business 1.0.0 | `PolyForm-Small-Business-1.0.0` | No (small firms may do anything) | No | Yes | Yes | Yes | Fails |
| CC BY-NC-SA 4.0 | `CC-BY-NC-SA-4.0` | Yes | Yes | Yes (share-alike) | Yes | **No** | Runner-up, not built for software |
| Commons Clause + Apache-2.0/MIT | base id + `WITH`-style notice; no SPDX id | No (only "Sell") | **No** | Yes | Yes | Base's | Fails partial reuse |
| Business Source License 1.1 | `BUSL-1.1` | Production use only | Yes until change | Yes | **No** (≤4 yrs) | No | Fails "forever" |
| Functional Source License 1.1 | `FSL-1.1-MIT`, `FSL-1.1-ALv2` | No (competing only) | Partly | Yes | **No** (2 yrs) | Yes | Fails |
| Elastic License 2.0 | `Elastic-2.0` | No (hosted service only) | No | Yes | Yes | Yes | Fails, aimed at SaaS |
| SSPL 1.0 | `SSPL-1.0` | No | No | Yes | Yes | Yes | Irrelevant to a desktop app |
| Prosperity 3.0.0 | not in SPDX 3.29 | Yes, after 30-day trial | Yes | Yes | Yes | Yes | Weaker; trial loophole |
| n8n Sustainable Use License | not in SPDX 3.29 | Partly (internal business use allowed) | Partly | Yes (internal) | Yes | No | Single-vendor, vague |
| AGPL-3.0 | `AGPL-3.0-only` | **No** | No | Yes (copyleft) | Yes | Yes | Open source; allows resale |
| MPL-2.0 | `MPL-2.0` | **No** | No | Yes (file-level) | Yes | Yes | Open source; allows resale |

### 3.2 PolyForm Noncommercial 1.0.0

- **Maintainer**: PolyForm Project. Drafted by software-licensing lawyers [observed, <https://polyformproject.org/about>]; Kyle E. Mitchell co-founded it [observed, <https://writing.kemitchell.com/2019/07/12/Polyform-1.0.0>], Heather Meeker announced the first draft [observed, <https://heathermeeker.com/2019/05/31/polyform-project-publishes-its-first-license-draft/>]. Released 2019-07-09 per SPDX. Text: <https://polyformproject.org/licenses/noncommercial/1.0.0> (plain text at `…/1.0.0.txt`).
- **Permits** [observed, quoting the licence]: a copyright licence "to do everything you might do with the software that would otherwise infringe the licensor's copyright in it for any permitted purpose"; a separate Distribution License, which "covers distributing the software with changes and new works"; a Changes and New Works License "to make changes and new works based on the software for any permitted purpose".
- **What counts as permitted** [observed]: "Any noncommercial purpose is a permitted purpose." Personal Uses: "Personal use for research, experiment, and testing for the benefit of public knowledge, personal study, private entertainment, hobby projects, amateur pursuits, or religious observance, without any anticipated commercial application". Noncommercial Organizations: "Use by any charitable organization, educational institution, public research organization, public safety or health organization, environmental protection organization, or government institution is use for a permitted purpose regardless of the source of funding or obligations resulting from the funding."
- **Bans commercial use outright?** Yes. Every grant is limited to a permitted purpose, and commercial purposes are not permitted. It is not limited to competing or hosted use.
- **Partial code reuse?** Yes [inference, strong]. Copying a function into a commercial product is making a "new work based on the software" for a non-permitted purpose, so it falls outside the licence. The Notices clause also attaches to "any part of the software" [observed]. Limits: copyright does not protect ideas or algorithms, very small snippets may not be protectable, and the licence preserves fair use ("These terms do not limit them" [observed]).
- **Share-alike?** No copyleft. Anyone who receives a copy must get "a copy of these terms or the URL for them" [observed, Notices], and nobody may sublicense ("These terms do not allow you to sublicense or transfer any of your licenses" [observed]). In practice your code stays under PolyForm NC in every fork [inference]. But a modifier is **not obliged to publish their changes' source**, and their own added code is theirs to licence.
- **Attribution / rebrand protection**: Recipients must get any line beginning `Required Notice:` that you provide [observed]. That makes your copyright line travel with every copy. There is **no trademark clause** [observed: no mention of "trademark" in any of the four PolyForm texts I downloaded]. Its "These terms do not imply any other licenses" line arguably means no trademark licence is implied [inference], but name protection needs a separate policy (section 6).
- **Patent grant**: Yes, plus patent defence, which ends the patent licence if the licensee claims the software infringes a patent [observed].
- **Termination**: 32-day cure period after the first written notice; "Otherwise, all your licenses end immediately." [observed]
- **Contributions**: The licence says nothing about them. Inbound contributions default to the same terms through GitHub ToS D.6 (section 4), which is a problem for you.
- **Drafting quality** [inference]: Short, plain-English, lawyer-drafted, widely mirrored, and the most-used standard noncommercial software licence. "Noncommercial" is not exhaustively defined, which is a known weak spot shared by every NC licence. Version 2.0.0 exists only as drafts (`2.0.0-pre.1` and `pre.2`, November 2025) [observed, <https://writing.kemitchell.com/2025/11/05/PolyForm-Noncommercial-2.0.0-pre.2>]. I found no final 2.0.0, so use 1.0.0.
- **GitHub detection** [observed]: GitHub uses `licensee`, which matches against the licences vendored from choosealicense.com. That list (checked via the licensee repo today) contains no PolyForm, BUSL, Elastic, FSL or CC-NC licence. **[inference]** The repo sidebar will therefore show a generic "View license"/"Other" label rather than the licence name. This is cosmetic. Keep `LICENSE` as the verbatim text, nothing added, and put the SPDX id in the README.

### 3.3 PolyForm Strict 1.0.0

[observed, <https://polyformproject.org/licenses/strict/1.0.0>] Copyright licence for a permitted purpose "other than distributing the software or making changes or new works based on the software." It has the same noncommercial definitions and patent grant as Noncommercial.

**Why not** [inference]: It prohibits modification, which directly contradicts "modify for own needs and contribute back". Opening a pull request means making changes. Use it only if you decide you want read-and-run but no forks. It has no SPDX id in list 3.29, so it would need a `LicenseRef-`.

### 3.4 PolyForm Shield 1.0.0

[observed, Shield text] "Any purpose is a permitted purpose, except for providing any product that competes with the software or any product the licensor or any of its affiliates provides using the software." Competition is defined broadly: "Goods and services compete even when provided free of charge. If you market a product as a practical substitute for the software or another product, it definitely competes."

**Why not**: A company could legally use Takyon's code in a commercial product that is not a launcher (for example, lifting the file index or clipboard code into a backup tool). That breaks "even part of the code for commercial gain". It is a reasonable licence if your real worry is only clones.

### 3.5 PolyForm Small Business 1.0.0

[observed, SPDX copy] Any purpose for organisations with "fewer than 100 total individuals working as employees and independent contractors" and "less than 1,000,000 USD (2019) total revenue in the prior tax year". **Why not**: a small company may sell or repackage Takyon outright.

### 3.6 CC BY-NC-SA 4.0 (runner-up)

- **Maintainer**: Creative Commons. Text: <https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode>.
- **NC definition** [observed, legal code §1(k)]: "NonCommercial means not primarily intended for or directed towards commercial advantage or monetary compensation."
- **Share-alike**: Adaptations must be licensed under the same or a compatible licence. This is stronger than PolyForm for keeping derivatives under NC terms, but it **does not require publishing source**.
- **Attribution**: Requires credit and indication of changes. That is good for anti-rebranding, though trademark is still excluded from the licence.
- **Why CC says no** [observed, <https://creativecommons.org/faq/#can-i-apply-a-creative-commons-license-to-software>]: "We recommend against using Creative Commons licenses for software." The reasons given are that CC licences "do not contain specific terms about the distribution of source code", do not address patents, and are not compatible with major software licences. CC's own FAQ also admits NC is fuzzy: "Whether a use is commercial will depend on the specifics of the situation and the intentions of the user." and "CC cannot advise you on what is and is not commercial use." [observed]
- **Patent grant**: None. CC 4.0 expressly excludes patent and trademark rights.
- **Verdict** [inference]: Use it only if share-alike matters more to you than a software-specific text and a patent licence. It is a fine choice for the `brand/` assets and `docs/` if you want those under a CC licence separately.

### 3.7 Commons Clause (on Apache-2.0 or MIT)

[observed, <https://commonsclause.com/>] It removes "the right to Sell the Software", where Sell means providing "for a fee or other consideration (including without limitation fees for hosting or consulting/ support services related to the Software), a product or service whose value derives, entirely or substantially, from the functionality of the Software." The FAQ says it "only forbids you from 'selling' the Commons Clause software itself."

**Why not**: Taking part of the code into a larger commercial product is explicitly allowed if the product's value does not derive "substantially" from Takyon. That fails your partial-reuse goal. It is also a bolt-on that sits awkwardly with the base licence's "you may sell" text. Redis used it in 2018 and later moved away from it.

### 3.8 Business Source License 1.1

[observed, <https://mariadb.com/bsl11/>] It grants "the right to copy, modify, create derivative works, redistribute, and make non-production use". Production use requires an Additional Use Grant or a commercial licence. Each version converts to the Change License on the Change Date or "the fourth anniversary of the first publicly available distribution of a specific version", and the Change License must be GPL-2.0-or-later or GPL-compatible.

**Why not**: (1) Each version **becomes open source within four years**, after which anyone may sell it, which fails "never". (2) "Production use" is a server concept; for a desktop launcher it is unclear whether daily personal use is production [inference]. (3) The mandatory GPL-compatible change licence works against your current "avoid GPL" stance. Adopters include MariaDB (MaxScale) and HashiCorp (2023).

### 3.9 Functional Source License 1.1 (FSL-1.1-MIT / FSL-1.1-ALv2)

[observed, template at <https://github.com/getsentry/fsl.software>] A Permitted Purpose is "any purpose other than a Competing Use", which means making the software available "in a commercial product or service that: 1. substitutes for the Software; 2. substitutes for any other product or service we offer…; or 3. offers the same or substantially similar functionality". It explicitly permits "your internal use and access". It has a trademark clause: "Except for displaying the License Details and identifying us as the origin of the Software, you have no right … to use our trademarks". Each version converts to MIT or Apache-2.0 after two years [observed, <https://fsl.software/>].

**Why not**: It allows non-competing commercial use and becomes MIT after two years. Its trademark clause is a good model to borrow. Adopters: Sentry, Codecov, GitButler [observed, <https://blog.sentry.io/introducing-the-functional-source-license-freedom-without-free-riding/>, <https://blog.gitbutler.com/gitbutler-is-now-fair-source>].

### 3.10 Elastic License 2.0

[observed, <https://www.elastic.co/licensing/elastic-license>] It has three limitations: no hosted or managed service providing "any substantial set of the features", no circumventing licence keys, and no removing notices. Trademarks: "Any use of the licensor's trademarks is subject to applicable law."

**Why not**: Commercial use, resale of copies and derivative products are all allowed. It targets cloud providers, which is irrelevant for a desktop launcher. (Elastic itself added AGPL as an option in 2024 [observed, <https://www.admin-magazine.com/News/Open-Source-AGPL-Added-as-License-Option-for-Elasticsearch/(tagID)/52>].)

### 3.11 SSPL 1.0

It is a copyleft licence that extends to everything used to offer the software as a service. It allows commercial use and resale. The OSI never approved it; it was withdrawn "when it became clear that the license would not be approved" [observed, <https://opensource.org/blog/the-sspl-is-not-an-open-source-license>]. **Why not**: it addresses SaaS, allows selling, and `scripts/check-licences.ts` already treats it as copyleft.

### 3.12 Prosperity Public License 3.0.0

[observed, <https://prosperitylicense.com/versions/3.0.0>] A project of License Zero (Kyle Mitchell). It lets you "use and share this software for noncommercial purposes for free", and it allows commercial use for a "thirty-day trial period". Contributions back under a standard public licence such as Apache 2.0 or MIT are not counted as commercial use. It is not on the SPDX list 3.29 [observed].

**Why not** [inference]: The trial is a built-in commercial window, and anyone can restart a "trial" with a new entity. It has fewer adopters than PolyForm and no SPDX id. PolyForm NC is effectively its successor.

### 3.13 n8n Sustainable Use License, and Fair Source

[observed, <https://github.com/n8n-io/n8n/blob/master/LICENSE.md>] "You may use or modify the software only for your own internal business purposes or for non-commercial or personal use." "You may distribute the software or provide it to others only if you do so free of charge for non-commercial purposes." The key terms are undefined [observed].

**Why not**: It explicitly permits internal business use (it does solve the "employee at work" issue, see 7.3) but leaves "internal business purposes" undefined. It was written for one vendor, has no SPDX id, and has no patent grant.

**Fair Source** [observed, <https://fair.io/about/>] requires software that "undergoes delayed Open Source publication (DOSP)". A forever-noncommercial licence therefore **does not qualify** as Fair Source, so do not use that label either.

### 3.14 AGPL-3.0 and MPL-2.0 (comparison)

Both are OSI-approved (SPDX flags above), so by definition both **allow commercial use, including selling copies**.

- **AGPL-3.0**: Strong copyleft, including network use. Anyone distributing a modified Takyon must publish the full source under AGPL. That actually satisfies "source stays fully public" better than any NC licence, but a company may still sell it, rebrand it (subject to trademark) or use it commercially. The common open-source route to discouraging commercial forks is **AGPL plus a paid commercial licence** (dual licensing), which needs a CLA. It is worth knowing about if the "open source" label ever matters more than the hard NC ban.
- **MPL-2.0**: File-level copyleft. Modified MPL files must stay MPL, but they can be combined into a proprietary product. It is the weakest fit.
- **About the repo's GPL ban** [observed, `scripts/check-licences.ts` and ADR-0005 line 30]: the ban covers *dependencies*, because a GPL dependency "would force Takyon to be GPL and eliminate the proprietary option". It says nothing about Takyon's own licence. The checker skips your own code: Rust path crates have `source === null` and are filtered out, and JS packages that are `private` or `@takyon/*` are skipped [observed]. Adopting PolyForm NC therefore does not trip `bun run check:licences`, and keeping the dependency ban remains correct, since a GPL dependency would make an NC licence on the combined binary impossible.

---

## 4. Contributions under a noncommercial licence

### 4.1 The default trap (inbound = outbound)

GitHub ToS D.6 [observed, <https://docs.github.com/en/site-policy/github-terms/github-terms-of-service>]: "Whenever you add Content to a repository containing notice of a license, you license that Content under the same terms… If you have a separate agreement to license that Content under different terms, such as a contributor license agreement, that agreement will supersede."

**Consequence** [inference, important]: Without a CLA, a contributor's PR comes to you **under PolyForm Noncommercial too**. You, as maintainer, would then hold only a noncommercial licence to their code. You could not:
- sell a commercial licence to a company that asks for one,
- relicense later (to open source, a proprietary product, or PolyForm 2.0),
- ship a paid edition,

unless you rewrite or remove every outside contribution. ADR-0005 keeps the proprietary option open on purpose, so this matters.

### 4.2 Options

| Mechanism | What you get | Fit |
|---|---|---|
| Inbound = outbound (default) | NC licence only | **Bad**, it locks you in |
| DCO 1.1 (`Signed-off-by`) | Contributor certifies the right to submit "under the open source license indicated in the file" [observed, <https://developercertificate.org/>] | **Bad**: the text presumes an *open source* licence, which Takyon will not have, and it grants no relicensing right |
| **CLA (licence grant)** | Contributor keeps copyright and grants you a broad, perpetual, irrevocable copyright and patent licence **with the right to sublicense and relicense** | **Recommended** |
| Copyright assignment | You own the contribution | Works, but heavier and deters contributors; unnecessary |

### 4.3 Recommendation

- Use an **individual CLA modelled on the Apache ICLA** (<https://www.apache.org/licenses/contributor-agreements.html>), with "the Foundation" replaced by "Nikhil Sah". Its licence-grant structure (perpetual, worldwide, non-exclusive, royalty-free, irrevocable, including the right to sublicense) gives you the relicensing freedom. Add one explicit sentence: "You agree that the Licensor may license your Contribution under any terms, including commercial and proprietary terms." Have a lawyer glance at it once; it is a one-time cost.
- **Tooling: CLA Assistant hosted at cla-assistant.io.** It is "provided by SAP as a free hosted offering" [observed, <https://github.com/cla-assistant/cla-assistant>]. You store the CLA as a GitHub Gist, the bot comments on each PR, and the PR status updates when the contributor signs.
- **Avoid `contributor-assistant/github-action`** (CLA Assistant Lite). The repo is archived and its maintainer says it is "no longer actively maintained" [observed, <https://github.com/contributor-assistant/github-action>].
- For small outside fixes before the CLA bot is in place, ask for a one-line PR comment agreeing to the CLA, or reimplement the fix yourself.

---

## 5. Compatibility with MIT and Apache-2.0 dependencies

**Allowed** [inference, standard practice]. MIT and Apache-2.0 permit use in software under any licence, including restrictive or proprietary ones. The NC terms apply only to *your* code; the dependencies stay under their own licences inside your distribution.

**Obligations you already have whenever you ship a binary** (this applies today, because `release.yml` publishes installers):
- MIT [observed, <https://opensource.org/license/mit>]: "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software."
- Apache-2.0 §4 [observed, <https://www.apache.org/licenses/LICENSE-2.0>]: give recipients "a copy of this License", keep "all copyright, patent, trademark, and attribution notices", and if a dependency ships a `NOTICE` file, include "a readable copy of the attribution notices contained within such NOTICE file".
- **Action**: generate a `THIRD_PARTY_NOTICES` file from `Cargo.lock` and `bun.lock` at release time and ship it inside the installer. `cargo-about` (Embark Studios, MIT/Apache) is the common Rust tool for this [inference: suggested tool; verify it fits your release script]. Because `check:licences` already walks both trees, extending it to also emit notices is a natural fit. This gap exists regardless of which licence you pick.
- Make clear in the README and NOTICE that PolyForm NC covers Takyon's own code only, and that third-party components remain under their own licences.

---

## 6. Trademark: the licence will not protect the name

- **No copyright licence stops a rebrand.** Under PolyForm NC, someone can make a noncommercial fork. The licence forces your copyright notice to travel with it, but says nothing about the product name. Conversely, it also does not stop a noncommercial fork from calling itself "Takyon", which confuses users.
- Comparison texts [observed]: Apache-2.0 §6: "This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work". FSL: "you have no right … to use our trademarks, trade names, service marks or product names". PolyForm: no trademark clause at all.
- **Recommendations**:
  1. Add `TRADEMARK.md`: "Takyon" and the Takyon mark identify official builds from github.com/Sah-Nikhil/Takyon. Modified or redistributed versions must use a different name and icon. They may say "based on Takyon" in descriptive text. No use in a way that implies endorsement.
  2. Use "Takyon™" in the README and installer. The USPTO says [observed, <https://www.uspto.gov/trademarks/basics/what-trademark>]: "You can use 'TM' for goods or 'SM' for services even if you haven't filed an application", and rights begin with use but are geographically limited without registration.
  3. **Do not use ® until the mark is registered.** In India, Trade Marks Act 1999 s.107 penalises falsely representing a mark as registered [observed, <https://www.advocatekhoj.com/library/bareacts/trademarks/107.php>; the penalty wording differs between sources, so check the current India Code text].
  4. If the project gains traction, register in your home country (Class 9, software) and consider the US or the Madrid system. **I did not run a clearance search.** Check for existing "Takyon"/"Tachyon" marks in Class 9 before filing or investing in the brand.
  5. The `brand/` assets (the locked mark) can stay "all rights reserved" and be excluded from the code licence in `NOTICE`, so nobody reads PolyForm NC as permission to reuse the logo.

---

## 7. Practical limits

### 7.1 Enforcement as a solo developer [inference]

- A licence is only as strong as your willingness to send a takedown. Realistic tools: a polite email first (PolyForm's 32-day cure clock starts at written notice), then a **DMCA notice** to GitHub, the Microsoft Store, winget, Chocolatey or a hosting provider. Takedowns are free and handle most small cases.
- Suing a company in another country is expensive. NC licences mainly deter good-faith companies, whose legal teams will not touch NC code, and they make takedowns of copies easy. They will not stop a determined bad actor in a jurisdiction you cannot reach.
- Keep evidence: git history with dates, release hashes (already produced by `bun run release --local`), and the `Required Notice:` line.

### 7.2 PolyForm NC lets institutions use it, whatever their funding [observed + inference]

The Noncommercial Organizations clause permits use by any "charitable organization, educational institution, public research organization, … or government institution … regardless of the source of funding". [inference] Because the Distribution License does not mention fees, a university or government body arguably could redistribute a modified Takyon, possibly even for a fee, as long as its *use* is within that clause. This is unlikely in practice for a launcher, but it is a real gap against "no entity may resell". It cannot be removed without editing the licence, which you should not do (see 7.4).

### 7.3 Using Takyon at work: decide this explicitly

[observed] The Personal Uses clause requires use "without any anticipated commercial application". A for-profit company is not a listed noncommercial organisation. PolyForm's own guidance points businesses to its separate Internal Use and Small Business licences instead (per search summary of <https://polyformproject.org/why-adopt-polyform>; I could not load that page directly).

[inference] So under plain PolyForm NC, **an employee, freelancer or contractor using Takyon as their everyday launcher at work is very likely not licensed.** A launcher's whole point is daily work use. That means the literal licence would make most professional users technically unlicensed, which (a) is probably not what you mean by "use for own individual needs", (b) makes careful companies ban it, and (c) makes the licence look hostile.

**Recommendation**: keep `LICENSE` as verbatim PolyForm NC and add a short, separate **additional permission**. The licence itself says it does not "prevent the licensor from granting licenses to anyone else" [observed]. Suggested wording (have a lawyer check it):

> Additional permission. In addition to the PolyForm Noncommercial License 1.0.0, Nikhil Sah grants any individual permission to install and run unmodified official releases of Takyon, or a version they have modified only for their own use, as a personal productivity tool, including in the course of their employment or business. This permission does not allow distributing, selling, sublicensing, hosting or bundling Takyon or any part of it, or using any part of its code in another product or service.

If you instead want work use banned, say so plainly in the README so nobody is surprised.

### 7.4 Other limits

- **Do not edit the PolyForm text.** Additions belong in separate files. An edited text loses the SPDX id, the GitHub and tooling recognition, and the drafting quality you chose it for [inference].
- **GitHub forking**: ToS D.5 [observed] lets every GitHub user "view and 'fork' your repositories … through the Service". That is a licence to copy *within GitHub only*; it does not grant commercial rights or use outside GitHub. You cannot prevent forks on GitHub, and you do not need to.
- **No source-disclosure duty**: no NC licence surveyed (including PolyForm NC and CC BY-NC-SA) forces someone who modifies Takyon to publish their changes. "Source stays fully public" is guaranteed only for *your* repo. Only copyleft open source licences (AGPL/GPL) force publication, and they allow commercial use.
- **Clean-room rewrites and ideas** are not protected by any copyright licence. Someone can build a commercial Spotlight clone inspired by Takyon's design.
- **Releases already published with no licence** are currently "all rights reserved" [observed, GitHub docs: "without a license, the default copyright laws apply"]. Adding PolyForm NC loosens that. Apply it going forward and state in the README that it covers all versions.

---

## 8. Recommendation

**Primary: PolyForm Noncommercial 1.0.0.** It is the only standard, lawyer-drafted, SPDX-listed software licence that bans all commercial use (including taking part of the code) while allowing study, modification, noncommercial redistribution and contributions, and it includes a patent licence.

**Runner-up: CC BY-NC-SA 4.0.** It is the best-known noncommercial licence and adds share-alike for derivatives, but Creative Commons recommends against it for software and it grants no patent rights.

### How each goal is met under the primary pick

| Goal | Met by |
|---|---|
| Anyone may use, study, modify for own needs | PolyForm NC copyright + changes licences (noncommercial) + additional permission for work use (7.3) |
| Contribute back freely | PolyForm NC distribution licence + CLA (section 4) |
| Source stays public | Your public repo; **gap**: forks need not publish source |
| No resale / repackaging | PolyForm NC: selling is a commercial purpose |
| No rebranding as own | `Required Notice:` line + `TRADEMARK.md` + ™ (section 6) |
| No commercial derivative, even partial | PolyForm NC changes licence limited to permitted purposes |
| Room to sell commercial licences or relicense later | CLA plus your sole authorship today (188 of 188 commits are by Nikhil Sah [observed, `git log`]) |

### Gaps no licence closes

1. Forks are not required to publish their source.
2. Institutions (charities, schools, governments) can use it and arguably redistribute it.
3. Ideas, UX and clean-room reimplementations remain unprotected.
4. "Noncommercial" has fuzzy edges in every licence that uses it.
5. Enforcement is on you, and cross-border action is costly.
6. GitHub forks are always possible.
7. Cannot be called open source or Fair Source.

### Files to add

| File | Content |
|---|---|
| `LICENSE` | Verbatim text of <https://polyformproject.org/licenses/noncommercial/1.0.0.txt>, nothing added, so tools match it |
| `NOTICE` | `Required Notice: Copyright (c) 2025-2026 Nikhil Sah (https://github.com/Sah-Nikhil/Takyon)` (adjust the start year to the first commit); scope ("applies to Takyon's own source; third-party components keep their licences, see THIRD_PARTY_NOTICES"); the additional work-use permission (7.3) if you adopt it; a note that `brand/` assets are all rights reserved and covered by `TRADEMARK.md`; and "For commercial licensing, contact …" |
| `CONTRIBUTING.md` | How to contribute; that every PR needs the CLA (bot-enforced); the git and commit conventions from `CLAUDE.md` |
| `CLA.md` (or a Gist linked by cla-assistant.io) | Individual CLA based on the Apache ICLA, granting the right to sublicense and relicense (section 4.3) |
| `TRADEMARK.md` | Name and logo policy (section 6) |
| `THIRD_PARTY_NOTICES` (generated, shipped in the installer) | MIT/Apache notices for every crate and npm package (section 5) |

Repo follow-ups (normal repo conventions; not done by me):
- Set `license = "PolyForm-Noncommercial-1.0.0"` in `apps/desktop/src-tauri/Cargo.toml` and `"license": "PolyForm-Noncommercial-1.0.0"` in the `package.json` files.
- Optionally show `LICENSE` in the NSIS installer via Tauri's bundle licence-file setting (verify the exact Tauri 2 config key first).
- Write a new ADR recording the decision, which closes the question ADR-0005 left open.
- Update README §Licence, the `CLAUDE.md` "Distribution is undecided" paragraph, and the header comment in `scripts/check-licences.ts`.
- Keep the GPL dependency ban: a GPL dependency would make the NC licence on the combined binary impossible.

### README wording (suggested)

```markdown
## Licence

Takyon is **source-available**, not open source. The code is public and you may
use, study and modify it, and share your changes, for **noncommercial purposes**
under the [PolyForm Noncommercial License 1.0.0](LICENSE)
(SPDX: `PolyForm-Noncommercial-1.0.0`). Individuals may also run it as their
everyday launcher at work; see [NOTICE](NOTICE).

You may not sell, resell, repackage or rebrand Takyon, or use any part of its
code in a commercial product or service. For a commercial licence, contact
<email>.

"Takyon" and the Takyon mark are trademarks of Nikhil Sah; forks must use a
different name. See [TRADEMARK.md](TRADEMARK.md).

Contributions are welcome and require signing the [CLA](CLA.md), which lets
the project keep licensing options open. Third-party components keep their own
licences; see THIRD_PARTY_NOTICES.
```

---

## Sources

- Open Source Definition: <https://opensource.org/osd>
- OSI on SSPL and "fauxpen" licences: <https://opensource.org/blog/the-sspl-is-not-an-open-source-license>
- FSF Free Software Definition: <https://www.gnu.org/philosophy/free-sw.html>
- SPDX License List 3.29.0 data: <https://github.com/spdx/license-list-data> (`json/licenses.json`), <https://spdx.org/licenses/PolyForm-Noncommercial-1.0.0.html>, <https://spdx.org/licenses/PolyForm-Small-Business-1.0.0.html>
- PolyForm texts: <https://polyformproject.org/licenses/noncommercial/1.0.0>, <https://polyformproject.org/licenses/strict/1.0.0>, <https://polyformproject.org/licenses/shield/1.0.0>, <https://polyformproject.org/licenses/small-business/1.0.0> (plain-text `.txt` variants downloaded); <https://polyformproject.org/about>
- PolyForm drafting and 2.0 drafts: <https://writing.kemitchell.com/2019/07/12/Polyform-1.0.0>, <https://heathermeeker.com/2019/05/31/polyform-project-publishes-its-first-license-draft/>, <https://writing.kemitchell.com/2025/11/05/PolyForm-Noncommercial-2.0.0-pre.2>
- Creative Commons FAQ and legal code: <https://creativecommons.org/faq/>, <https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode>
- Commons Clause: <https://commonsclause.com/>
- Business Source License 1.1: <https://mariadb.com/bsl11/>
- Functional Source License: <https://fsl.software/>, <https://github.com/getsentry/fsl.software>, <https://blog.sentry.io/introducing-the-functional-source-license-freedom-without-free-riding/>, <https://blog.gitbutler.com/gitbutler-is-now-fair-source>
- Fair Source: <https://fair.io/about/>
- Elastic License 2.0: <https://www.elastic.co/licensing/elastic-license>; Elastic AGPL option: <https://www.admin-magazine.com/News/Open-Source-AGPL-Added-as-License-Option-for-Elasticsearch/(tagID)/52>
- Prosperity 3.0.0: <https://prosperitylicense.com/versions/3.0.0>
- n8n Sustainable Use License: <https://github.com/n8n-io/n8n/blob/master/LICENSE.md>
- GitHub ToS D.5 and D.6: <https://docs.github.com/en/site-policy/github-terms/github-terms-of-service>
- GitHub licence detection: <https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository>; licensee vendored list: <https://github.com/licensee/licensee/tree/main/vendor/choosealicense.com/_licenses>
- DCO 1.1: <https://developercertificate.org/>
- Apache License 2.0: <https://www.apache.org/licenses/LICENSE-2.0>; Apache CLAs: <https://www.apache.org/licenses/contributor-agreements.html>
- MIT License: <https://opensource.org/license/mit>
- CLA Assistant: <https://github.com/cla-assistant/cla-assistant>, <https://github.com/contributor-assistant/github-action>
- USPTO trademark basics: <https://www.uspto.gov/trademarks/basics/what-trademark>
- India Trade Marks Act 1999 s.107: <https://www.advocatekhoj.com/library/bareacts/trademarks/107.php>, official: <https://www.indiacode.nic.in/bitstream/123456789/1993/1/a199947.pdf>
- Repo observations: `docs/adr/0005-no-bundled-browser-for-web-search.md` line 30, `scripts/check-licences.ts`, `README.md` §Licence, `git log`
