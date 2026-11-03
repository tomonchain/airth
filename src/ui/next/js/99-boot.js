async function boot() {
  document.addEventListener("click", () => { if (next.menuOpen) { next.menuOpen = false; renderTop(); } });
  window.addEventListener("hashchange", onRoute);
  if (!next.unauthorized) {
    try {
      next.realms = await api("realms");
      let last = null;
      try { last = localStorage.getItem("airth-next-realm"); } catch {}
      const known = (n) => next.realms.some((r) => r.name === n && !r.archived);
      next.realm = known(last) ? last : (next.realms.find((r) => r.default && !r.archived) || next.realms.find((r) => !r.archived) || {}).name || null;
      // The account at the header's right.
      api("owner").then((o) => { next.owner = o; renderTop(); }).catch(() => {});
    } catch (e) {
      if (e instanceof AuthError) next.unauthorized = true;
    }
  }
  onRoute();
}
boot();
