// ══════════════════════════════════════════════════════
// Ariadne · comportamiento compartido del sitio de documentación.
//
// Dos cosas, las dos opcionales por diseño: si este módulo no carga (CDN
// caído, bloqueador), `.js-ready` nunca se agrega, todo el contenido ya
// es visible por default (ver `.reveal` en ariadne-docs.css) y el índice
// simplemente no aparece. La página se lee igual.
//
// El índice se arma solo con los h2/h3 que tengan id, igual que
// doc-prose.js de iroFactory (shared/components/doc-prose).
// ══════════════════════════════════════════════════════

import { inView } from "https://cdn.jsdelivr.net/npm/motion@11/+esm";

document.body.classList.add("js-ready");

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// --- índice de la página, con la sección que se está leyendo marcada ---
export function mountToc({ root = ".doc", toc = ".doc-toc" } = {}) {
  document.querySelectorAll(root).forEach((doc) => {
    const box = doc.closest(".doc-layout")?.querySelector(toc) || doc.querySelector(toc);
    if (!box) return;
    const heads = [...doc.querySelectorAll("h2[id], h3[id]")];
    if (!heads.length) return;

    box.innerHTML =
      "<p>En esta página</p><ol>" +
      heads
        .map(
          (h) =>
            `<li class="${h.tagName === "H3" ? "is-h3" : ""}"><a href="#${h.id}">${h.textContent}</a></li>`,
        )
        .join("") +
      "</ol>";

    const links = new Map(heads.map((h) => [h, box.querySelector(`a[href="#${CSS.escape(h.id)}"]`)]));
    let current = null;
    // La sección actual es el último encabezado que ya pasó por arriba del
    // tercio inferior de la pantalla.
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (e.isIntersecting) current = e.target;
        links.forEach((a, h) => a?.setAttribute("aria-current", String(h === current)));
      },
      { rootMargin: "0px 0px -66% 0px" },
    );
    heads.forEach((h) => io.observe(h));
  });
}

// --- reveals sutiles al entrar en viewport, escalonados de a tres ---
export function mountReveals(selector = ".reveal") {
  const items = document.querySelectorAll(selector);
  if (reduced) {
    items.forEach((el) => el.classList.add("in"));
    return;
  }
  items.forEach((el, i) => {
    inView(el, () => setTimeout(() => el.classList.add("in"), (i % 3) * 60), { amount: 0.2 });
  });
  // Red de seguridad: si inView nunca dispara para algún elemento (impresión,
  // un scroll poco convencional), el contenido no debe quedar invisible para
  // siempre.
  setTimeout(() => items.forEach((el) => el.classList.add("in")), 4000);
}

mountToc();
mountReveals();
