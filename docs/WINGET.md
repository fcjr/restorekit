# winget (shelved)

Plan for publishing to the Windows Package Manager. Not wired up yet — it
needs a dedicated token and a one-time bootstrap (below). The config in this
doc was fully implemented and locally verified once (manifests generated and
schema-validated), then backed out until the prerequisites exist; it can be
restored as-is.

## How winget distribution works

There is no bucket or tap to own. Every package lives as YAML manifests in
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs), and every
new version is a pull request against it, opened from a fork. Microsoft's bots
validate the PR (URL reachability, Defender scan) and moderators merge it.
First-time submissions of a new package go through human moderation and can
take days; version updates are usually auto-merged within hours. Unsigned NSIS
installers occasionally get pulled into manual review — friction, not a
blocker.

## Package identifiers (decided)

| Package | Identifier | Rationale |
| --- | --- | --- |
| Desktop app | `fcjr.RestoreKit` | Plain product name goes to the app, matching the Homebrew cask reservation. |
| CLI | `fcjr.RestoreKit.CLI` | `.CLI` suffix segment per the `AgileBits.1Password` / `AgileBits.1Password.CLI` precedent. |

Publisher segment is `fcjr` (the identifier), display `PublisherName` is
"Frank Chiarulli Jr.". Identifiers match case-insensitively, so lowercase
`fcjr` is fine (`sharkdp.bat`, `ajeetdsouza.zoxide` precedent).

## Prerequisites

1. **A fork of `microsoft/winget-pkgs`.** Both publish paths push manifest
   branches to it. `fcjr/winget-pkgs` exists, but see the machine-account note.
2. **A classic PAT with the `public_repo` scope**, stored as the
   `WINGET_GITHUB_TOKEN` secret. It must be classic: opening the PR is an API
   write on *Microsoft's* repo, and fine-grained PATs can only be granted on
   repos you own — a fine-grained token pushes to the fork fine, then 403s on
   PR creation. This is why goreleaser, winget-releaser, and Komac all require
   a classic token.
3. **Preferably a machine account.** `public_repo` grants write to all public
   repos *of the token's owner*, so generate the PAT from a bot account (GitHub
   ToS allows one free machine account per person) that owns nothing but its
   `winget-pkgs` fork. A leaked token then reaches only a throwaway fork and
   moderated PRs. If a bot is used, its fork replaces `fcjr/winget-pkgs` in the
   config below.

## CLI: goreleaser (release.yml)

goreleaser OSS has native winget support. Verified locally with a snapshot
release: it writes the three manifests at the correct sharded path
(`manifests/f/fcjr/RestoreKit/CLI/<version>/`), declares the zip as a
portable-in-zip installer, and sets `PortableCommandAlias: restorekit`, so the
CLI lands on PATH under its real name. goreleaser writes complete manifests,
so the very first release can bootstrap the package itself — no manual `new`
needed; the initial PR just sits in new-package moderation. (goreleaser's
winget config has no `moniker` field; not needed given the command alias.)

Stanza for `.goreleaser.yaml` (after `scoops:`):

```yaml
winget:
  # Same naming rationale as the cask/scoop: `fcjr.RestoreKit` is reserved for
  # the desktop app (published from release-app.yml via wingetcreate). Each
  # release pushes manifests to the winget-pkgs fork and opens a PR against
  # microsoft/winget-pkgs.
  - name: RestoreKit CLI
    publisher: Frank Chiarulli Jr.
    package_identifier: fcjr.RestoreKit.CLI
    short_description: "DFU-restore Apple Silicon Macs from the command line"
    license: Apache-2.0
    homepage: "https://github.com/fcjr/restorekit"
    publisher_url: "https://github.com/fcjr"
    publisher_support_url: "https://github.com/fcjr/restorekit/issues"
    release_notes_url: "https://github.com/fcjr/restorekit/releases/tag/v{{ .Version }}"
    tags: [dfu, macos, restore, apple-silicon]
    repository:
      owner: fcjr            # or the machine account
      name: winget-pkgs
      token: "{{ .Env.WINGET_GITHUB_TOKEN }}"
      branch: "fcjr.RestoreKit.CLI-{{ .Version }}"
      pull_request:
        enabled: true
        base:
          owner: microsoft
          name: winget-pkgs
          branch: master
```

And in `release.yml`, alongside the other goreleaser env vars:

```yaml
          WINGET_GITHUB_TOKEN: ${{ secrets.WINGET_GITHUB_TOKEN }}
```

## Desktop app: wingetcreate (release-app.yml)

`wingetcreate update` bumps the existing manifests to a new installer URL and
submits the PR — but it requires the package to already exist upstream, so the
first version must be bootstrapped by hand:

```sh
wingetcreate new https://github.com/fcjr/restorekit/releases/download/v<ver>/RestoreKit_<ver>_x64-setup.exe
# identifier fcjr.RestoreKit, moniker restorekit, fill metadata, submit
```

After that PR merges, this step in the `build-app` job (Windows runner, after
tauri-action has uploaded the installer asset) keeps it current:

```yaml
      # Bump the fcjr.RestoreKit winget manifests to this release's NSIS
      # installer and submit the PR to microsoft/winget-pkgs. `update` requires
      # the package to already exist upstream — the first version was
      # bootstrapped manually with `wingetcreate new`.
      - name: Publish to winget
        if: runner.os == 'Windows'
        shell: pwsh
        env:
          WINGET_GITHUB_TOKEN: ${{ secrets.WINGET_GITHUB_TOKEN }}
        run: |
          $version = $env:GITHUB_REF_NAME.TrimStart("v")
          $url = "https://github.com/fcjr/restorekit/releases/download/v${version}/RestoreKit_${version}_x64-setup.exe"
          Invoke-WebRequest https://aka.ms/wingetcreate/latest -OutFile wingetcreate.exe
          .\wingetcreate.exe update fcjr.RestoreKit --version $version --urls $url --submit --token $env:WINGET_GITHUB_TOKEN
```

## Checklist to un-shelve

- [ ] Create the machine account and fork `microsoft/winget-pkgs` under it
      (or settle for the `fcjr/winget-pkgs` fork).
- [ ] Classic PAT (`public_repo`) from that account → `WINGET_GITHUB_TOKEN`
      secret on `fcjr/restorekit`.
- [ ] Add the goreleaser stanza + env line (CLI) and the workflow step (app)
      from this doc; fix the fork `owner` if using the bot.
- [ ] Tag a release — the CLI package bootstraps itself via goreleaser.
- [ ] After that release's installer is up, run `wingetcreate new` once for
      `fcjr.RestoreKit`; expect new-package moderation to take days for both.
- [ ] Add `winget install` lines to README + DEPLOYMENT.md once merged.
