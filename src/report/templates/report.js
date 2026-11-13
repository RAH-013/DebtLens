(() => {
  "use strict";

  function normalize(value) {
    return String(value ?? "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .trim();
  }

  function parseNumeric(text) {
    const cleaned = String(text ?? "")
      .replace(",", ".")
      .trim();
    const num = parseFloat(cleaned);
    return Number.isNaN(num) ? null : num;
  }

  function scoreToThermoHue(score) {
    const clamped = Math.min(100, Math.max(0, score));
    let hue;

    if (clamped <= 35) {
      // 0% (152° verde esmeralda) -> 35% (45° amarillo ámbar)
      hue = 152 - (clamped / 35) * (152 - 45);
    } else if (clamped <= 65) {
      // 35% (45° amarillo) -> 65% (22° naranja)
      hue = 45 - ((clamped - 35) / 30) * (45 - 22);
    } else {
      // 65% (22° naranja) -> 100% (350° rojo carmesí)
      hue = 22 - ((clamped - 65) / 35) * 32;
    }

    return Math.round((hue + 360) % 360);
  }

  function initSummaryCards() {
    const scoreEl = document.getElementById("debt-score-value");
    const debtFill = document.getElementById("debt-meter-fill");
    const debtLevelEl = document.getElementById("debt-level-badge");
    const debtCard = document.getElementById("card-debt");
    const healthPill = document.getElementById("debt-health-pill");

    const score = scoreEl ? parseNumeric(scoreEl.textContent) : null;

    if (debtCard && healthPill) {
      let stateClass = "is-moderate";

      if (score !== null) {
        const clampedScore = Math.min(100, Math.max(0, score));
        const hue = scoreToThermoHue(clampedScore);

        debtCard.style.setProperty("--thermo-hue", String(hue));
        healthPill.style.setProperty("--thermo-hue", String(hue));

        if (debtFill) {
          debtFill.style.width = `${clampedScore}%`;
        }

        if (clampedScore >= 70) stateClass = "is-critical";
        else if (clampedScore >= 45) stateClass = "is-high";
        else if (clampedScore >= 25) stateClass = "is-moderate";
        else stateClass = "is-low";
      } else if (debtLevelEl) {
        const lvl = normalize(debtLevelEl.textContent);
        if (lvl.includes("crit")) stateClass = "is-critical";
        else if (lvl.includes("alt") || lvl.includes("high"))
          stateClass = "is-high";
        else if (lvl.includes("baj") || lvl.includes("low"))
          stateClass = "is-low";
      }

      debtCard.classList.remove(
        "is-low",
        "is-moderate",
        "is-high",
        "is-critical",
      );
      healthPill.classList.remove(
        "is-low",
        "is-moderate",
        "is-high",
        "is-critical",
      );
      debtCard.classList.add(stateClass);
      healthPill.classList.add(stateClass);
    }

    const affectedEl = document.getElementById("affected-pct-value");
    const filesFill = document.getElementById("files-meter-fill");
    if (affectedEl && filesFill) {
      const pct = parseNumeric(affectedEl.textContent);
      if (pct !== null) {
        filesFill.style.width = `${Math.min(100, Math.max(0, pct))}%`;
      }
    }

    const syntaxValEl = document.getElementById("syntax-errors-value");
    const syntaxCard = document.getElementById("card-syntax");
    const syntaxBadge = document.getElementById("syntax-status-badge");
    if (syntaxValEl && syntaxCard && syntaxBadge) {
      const errors = parseInt(String(syntaxValEl.textContent ?? "").trim(), 10);
      syntaxCard.classList.remove("is-critical", "is-emerald");

      if (!Number.isNaN(errors) && errors > 0) {
        syntaxCard.classList.add("is-critical");
        syntaxBadge.textContent = "⚠ Con errores";
      } else {
        syntaxCard.classList.add("is-emerald");
        syntaxBadge.textContent = "✓ Limpio";
      }
    }

    const engineValEl = document.getElementById("ai-engine-value");
    const engineCard = document.getElementById("card-engine");
    const liveDot = document.getElementById("ai-live-dot");
    if (engineValEl && engineCard) {
      const hasGemini = normalize(engineValEl.textContent).includes("gemini");
      engineCard.classList.toggle("is-gemini", hasGemini);
      engineCard.classList.toggle("is-static-only", !hasGemini);
      if (liveDot) {
        liveDot.classList.toggle("is-offline", !hasGemini);
      }
    }
  }

  function init() {
    initSummaryCards();

    const themeButtons = document.querySelectorAll(".theme-btn");
    const STORAGE_KEY = "debtlens-theme-mode";

    function readStorage(key) {
      try {
        return window.localStorage.getItem(key);
      } catch {
        return null;
      }
    }

    function writeStorage(key, value) {
      try {
        if (value === null) {
          window.localStorage.removeItem(key);
        } else {
          window.localStorage.setItem(key, value);
        }
      } catch {
        // Ignorar restricciones en file://
      }
    }

    function applyTheme(theme) {
      const validTheme =
        theme === "light" || theme === "dark" ? theme : "system";

      if (validTheme === "system") {
        document.documentElement.removeAttribute("data-theme");
        writeStorage(STORAGE_KEY, null);
      } else {
        document.documentElement.setAttribute("data-theme", validTheme);
        writeStorage(STORAGE_KEY, validTheme);
      }

      themeButtons.forEach((btn) => {
        const isActive = btn.dataset.themeVal === validTheme;
        btn.classList.toggle("active", isActive);
        btn.setAttribute("aria-checked", isActive ? "true" : "false");
      });
    }

    const savedTheme = readStorage(STORAGE_KEY) || "system";
    applyTheme(savedTheme);

    themeButtons.forEach((btn) => {
      btn.addEventListener("click", () => {
        applyTheme(btn.dataset.themeVal);
      });
    });

    const originFilter = document.getElementById("origin-filter");
    const severityFilter = document.getElementById("severity-filter");
    const searchFilter = document.getElementById("search-filter");
    const findingsContainer = document.getElementById("findings");
    const visibleCount = document.getElementById("visible-count");
    const resultBadgeCount = document.getElementById("result-badge-count");
    const emptyState = document.getElementById("empty-state");
    const emptyTitle = document.getElementById("empty-title");
    const emptyDescription = document.getElementById("empty-description");
    const clearButton = document.getElementById("clear-filters");
    const emptyClearButton = document.getElementById("empty-clear");

    if (!findingsContainer || !severityFilter || !searchFilter) {
      return;
    }

    const cards = Array.from(findingsContainer.querySelectorAll(".finding"));

    if (cards.length === 0) {
      findingsContainer.innerHTML = "";
    }

    function getSearchableText(card) {
      return normalize(card.textContent);
    }

    function updateFindings() {
      const origin = originFilter ? originFilter.value : "all";
      const severity = severityFilter.value;
      const query = normalize(searchFilter.value);
      let visible = 0;

      for (const card of cards) {
        const cardOrigin = (card.dataset.origin || "static").toLowerCase();
        const cardSeverity = (card.dataset.severity || "").toLowerCase();

        const matchesOrigin = origin === "all" || cardOrigin === origin;
        const matchesSeverity = severity === "all" || cardSeverity === severity;
        const matchesText =
          query.length === 0 || getSearchableText(card).includes(query);
        const matches = matchesOrigin && matchesSeverity && matchesText;

        card.hidden = !matches;

        if (matches) {
          visible++;
        }
      }

      const total = cards.length;
      const hasFilters =
        origin !== "all" || severity !== "all" || query.length > 0;
      const noResults = visible === 0;

      if (emptyState) emptyState.hidden = !noResults;
      if (clearButton) clearButton.hidden = !hasFilters;

      if (total === 0) {
        if (emptyTitle) emptyTitle.textContent = "No hay hallazgos detectados";
        if (emptyDescription) {
          emptyDescription.textContent =
            "El informe no contiene resultados para las reglas ejecutadas.";
        }
        if (emptyClearButton) emptyClearButton.hidden = true;
        if (visibleCount) {
          visibleCount.textContent = "El análisis no registró hallazgos.";
        }
      } else if (noResults) {
        if (emptyTitle) emptyTitle.textContent = "No se encontraron resultados";
        if (emptyDescription) {
          emptyDescription.textContent =
            "Ningún hallazgo coincide con los filtros actuales. Cambia los filtros o prueba otros términos.";
        }
        if (emptyClearButton) emptyClearButton.hidden = false;
        if (visibleCount) {
          visibleCount.textContent =
            "No hay resultados para los filtros seleccionados.";
        }
      } else {
        if (emptyClearButton) emptyClearButton.hidden = true;
        if (visibleCount) {
          visibleCount.textContent = hasFilters
            ? `Mostrando ${visible} de ${total} hallazgos.`
            : `${total} hallazgos disponibles.`;
        }
      }

      if (resultBadgeCount) {
        resultBadgeCount.textContent = String(visible);
      }
    }

    function clearFilters() {
      if (originFilter) originFilter.value = "all";
      severityFilter.value = "all";
      searchFilter.value = "";
      updateFindings();
      searchFilter.focus();
    }

    if (originFilter) originFilter.addEventListener("change", updateFindings);
    severityFilter.addEventListener("change", updateFindings);
    searchFilter.addEventListener("input", updateFindings);
    if (clearButton) clearButton.addEventListener("click", clearFilters);
    if (emptyClearButton) {
      emptyClearButton.addEventListener("click", clearFilters);
    }

    for (const [index, card] of cards.entries()) {
      card.style.animationDelay = `${Math.min(index * 45, 360)}ms`;
    }

    updateFindings();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
