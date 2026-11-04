// Copy hash buttons.
document.querySelectorAll(".copy").forEach((button) => {
  button.addEventListener("click", async () => {
    const key = button.dataset.copy;
    const code = document.querySelector(`[data-hash-${key}]`);
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code.textContent.trim());
      const old = button.innerHTML;
      button.innerHTML =
        '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12.5 9.5 18 20 6.5"/></svg>';
      setTimeout(() => (button.innerHTML = old), 1400);
    } catch {
      // Clipboard unavailable — select the text instead.
      const range = document.createRange();
      range.selectNodeContents(code);
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
  });
});

document.querySelector("[data-year]").textContent = new Date().getFullYear();

document.querySelectorAll("[data-lang-set]").forEach((link) => {
  link.addEventListener("click", () => {
    try {
      localStorage.setItem("ezcap-lang", link.dataset.langSet);
    } catch {
      // Private mode can reject storage. The link still opens the other page.
    }
  });
});

// Local preview: when CI has not replaced the @@...@@ tokens yet, pull the
// latest release version and asset links from the GitHub API.
const TOKEN = "@@";
const tokenNodes = [
  ...document.querySelectorAll("[data-version]"),
  ...document.querySelectorAll("[data-hash-setup], [data-hash-zip]"),
  ...document.querySelectorAll("[data-size-setup], [data-size-zip]"),
  ...document.querySelectorAll("[data-dl-setup], [data-dl-zip]"),
];
const hasTokens = tokenNodes.some(
  (node) =>
    node.textContent.includes(TOKEN) ||
    (node.getAttribute("href") || "").includes(TOKEN)
);

const RELEASES_PAGE = "https://github.com/machine202502/ezcap/releases/latest";

// Point every download button at the release page (used while the exact
// asset URLs are unknown).
function fallbackLinks() {
  document
    .querySelectorAll("[data-dl-setup], [data-dl-zip]")
    .forEach((a) => (a.href = RELEASES_PAGE));
}

function hideRemainingTokens() {
  for (const node of tokenNodes) {
    const isLink = node.hasAttribute("href");
    const filled = isLink
      ? !(node.getAttribute("href") || "").includes(TOKEN)
      : !node.textContent.includes(TOKEN);
    if (filled) continue;
    // Meta rows (size/hash) live in their own wrapper div; hide the row.
    const row = node.closest(".meta > div") || node.closest(".pill");
    (row || node).style.display = "none";
  }
}

if (hasTokens) {
  fallbackLinks();
  fetch("https://api.github.com/repos/machine202502/ezcap/releases/latest")
    .then((response) => (response.ok ? response.json() : null))
    .then((release) => {
      let version = "";
      let setup = null;
      let zip = null;
      if (release) {
        version = (release.tag_name || "").replace(/^v/, "");
        setup = release.assets.find((a) => a.name.endsWith("-setup.exe"));
        zip = release.assets.find(
          (a) => a.name.endsWith(".zip") && !a.name.endsWith("-setup.exe")
        );
      }

      if (version) {
        document.querySelectorAll("[data-version]").forEach((n) => {
          if (n.closest("a")) n.textContent = `-${version}-`;
          else n.textContent = n.textContent.split("@@VERSION@@").join(version);
        });
        if (setup) {
          document
            .querySelectorAll("[data-dl-setup]")
            .forEach((a) => (a.href = setup.browser_download_url));
        }
        if (zip) {
          document
            .querySelectorAll("[data-dl-zip]")
            .forEach((a) => (a.href = zip.browser_download_url));
        }
      } else {
        // No release to name files after: use generic button labels.
        document.querySelectorAll("[data-dl-setup], [data-dl-zip]").forEach((a) => {
          if (a.dataset.fallback) a.textContent = a.dataset.fallback;
        });
      }
      // The API does not expose file hashes: hide the rows CI did not fill.
      for (const [asset, sizeKey, hashKey] of [
        [setup, "size-setup", "hash-setup"],
        [zip, "size-zip", "hash-zip"],
      ]) {
        const sizeNode = document.querySelector(`[data-${sizeKey}]`);
        const hashNode = document.querySelector(`[data-${hashKey}]`);
        if (!sizeNode || !hashNode) continue;
        if (sizeNode.textContent.includes(TOKEN)) {
          const unit = document.documentElement.dataset.sizeUnit || "MB";
          sizeNode.textContent = asset
            ? `${(asset.size / 1048576).toFixed(1)} ${unit}`
            : "—";
        }
        if (hashNode.textContent.includes(TOKEN)) {
          hashNode.closest("div").style.display = "none";
        }
      }
      hideRemainingTokens();
    })
    .catch(hideRemainingTokens);
}
