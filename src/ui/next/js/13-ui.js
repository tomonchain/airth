// The interface's building blocks (decision 72): one page header, three
// page layouts, and the parts every page shares. A page is one of the three
// layouts, never a layout of its own.

/** The page header: label, title, summary, actions; the same place on
    every page. */
function PageHeader({ label, title, summary, actions }) {
  return html`<header class="page-header">
    <div class="page-heading">
      ${label && html`<div class="page-label">${label}</div>`}
      <h1 class="page-title">${title}</h1>
      ${summary && html`<p class="page-summary">${summary}</p>`}
    </div>
    ${actions && html`<div class="page-actions">${actions}</div>`}
  </header>`;
}

/** Reading: one column under the header, 880 px at most. `kind` names the
    page for its own styles, never for a layout of its own. */
function Reading({ header, kind, children }) {
  return html`<div class=${`layout reading ${kind || ""}`} data-layout="reading">
    <${PageHeader} ...${header} />
    <div class="layout-body">${children}</div>
  </div>`;
}

/** Board: the panel's full width under the header. */
function Board({ header, kind, children }) {
  return html`<div class=${`layout board ${kind || ""}`} data-layout="board">
    <${PageHeader} ...${header} />
    <div class="layout-body">${children}</div>
  </div>`;
}

/** Workspace: a list, the work, and its details, the same widths on every
    workspace page. */
function Workspace({ header, list, main, details }) {
  return html`<div class="layout workspace" data-layout="workspace">
    <${PageHeader} ...${header} />
    <div class="workspace-panes">
      ${list && html`<aside class="ws-list">${list}</aside>`}
      <section class="ws-main">${main}</section>
      ${details && html`<aside class="ws-details">${details}</aside>`}
    </div>
  </div>`;
}

/** A part not built yet: what it will do (IN_DEVELOPMENT's texts). */
function InDevelopment({ part }) {
  const [title, ...lines] = IN_DEVELOPMENT[part];
  return html`<section class="in-dev" aria-label=${`${title}, in development`}>
    <span class="in-dev-tag">In development</span>
    <strong>${title}</strong>
    ${lines.map((l) => html`<p>${l}</p>`)}
  </section>`;
}

/** A page still built by the older DOM builder, shown as it is until it
    moves to components. */
function Legacy({ node }) {
  const box = useRef(null);
  useLayoutEffect(() => {
    if (box.current) box.current.replaceChildren(node);
  }, [node]);
  return html`<div class="legacy" ref=${box}></div>`;
}
