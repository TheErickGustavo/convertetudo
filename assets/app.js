const state = {
  files: [],
  formats: [],
  to: "txt",
  selected: null,
  paletteFor: null,
  view: "page",
  seq: 1,
};

const $ = (id) => document.getElementById(id);
const desk = $("desk");
const stage = $("stage");
const jobs = $("jobs");
const empty = $("empty");
const live = $("live");

function say(msg) { live.textContent = msg; }

function extOf(name) {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

function alias(id) {
  const map = {
    text: "txt", markdown: "md", mdown: "md", htm: "html", xhtml: "html",
    docm: "docx", dotx: "docx", xlsm: "xlsx", tsv: "csv", azw: "mobi", azw3: "mobi",
    prc: "mobi", latex: "tex", oxps: "xps", sxw: "odt",
  };
  return map[id] || id;
}

function fmt(id) {
  return state.formats.find((f) => f.id === id);
}

function label(id) {
  return (fmt(id) && fmt(id).label) || id.toUpperCase();
}

function sizeOf(n) {
  if (n < 1024) return n + " B";
  if (n < 1024 * 1024) return (n / 1024).toFixed(n < 10 * 1024 ? 1 : 0).replace(".", ",") + " KB";
  return (n / 1024 / 1024).toFixed(1).replace(".", ",") + " MB";
}

async function boot() {
  const res = await fetch("/api/formats");
  const data = await res.json();
  state.formats = data.formats;
  const join = (list) => list.map((f) => f.label).join(", ");
  $("reads").textContent = join(state.formats.filter((f) => f.read));
  $("writes").textContent = join(state.formats.filter((f) => f.write));
  renderPalette();
  render();
}

function addFiles(list) {
  const incoming = [...list].filter((f) => f && f.size >= 0);
  if (!incoming.length) return;
  for (const file of incoming) {
    if (file.size > 80 * 1024 * 1024) {
      say(file.name + " passa de 80 MB.");
      continue;
    }
    const from = alias(extOf(file.name) || "txt");
    state.files.push({
      id: state.seq++,
      file,
      name: file.name,
      size: file.size,
      from,
      to: state.to,
      status: "na fila",
      kind: "wait",
      error: "",
      blob: null,
      outName: "",
      html: "",
      text: "",
      warnings: [],
    });
  }
  if (!state.selected && state.files.length) state.selected = state.files[0].id;
  render();
  const last = state.files[state.files.length - 1];
  if (last) preview(last.id);
}

function showBlank(message) {
  const sheet = $("sheet");
  sheet.classList.remove("filled");
  sheet.innerHTML = "";
  const p = document.createElement("p");
  p.className = "blank";
  p.id = "blankCopy";
  p.textContent = message || "A prova do texto aparece aqui.";
  sheet.appendChild(p);
  $("raw").hidden = true;
  $("sheet").hidden = false;
  $("proofName").textContent = "Prova";
  $("warns").hidden = true;
}

function render() {
  const has = state.files.length > 0;
  empty.hidden = has;
  jobs.hidden = !has;
  $("convert").disabled = !has || $("convert").dataset.busy === "1";
  $("formatName").textContent = label(state.to);
  $("zip").hidden = state.files.filter((f) => f.blob).length < 2;
  jobs.innerHTML = "";
  for (const f of state.files) {
    const li = document.createElement("li");
    li.className = "job-row" + (f.id === state.selected ? " on" : "");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "job";
    btn.innerHTML = `
      <span class="job-name"><strong></strong><span></span></span>
      <span class="job-flow"></span>
      <span class="status"></span>`;
    btn.querySelector("strong").textContent = f.name;
    btn.querySelector(".job-name span").textContent = sizeOf(f.size);
    const flow = btn.querySelector(".job-flow");
    flow.append(document.createTextNode(label(f.from) + " → "));
    const b = document.createElement("b");
    b.textContent = label(f.to);
    flow.append(b);
    const st = btn.querySelector(".status");
    st.textContent = f.error || f.status;
    st.className = "status" + (f.kind === "ok" ? " ok" : f.kind === "bad" ? " bad" : "");
    btn.addEventListener("click", () => {
      state.selected = f.id;
      render();
      if (!f.html && !f.text) preview(f.id);
      else showProof(f);
    });
    const dl = document.createElement("button");
    dl.type = "button";
    dl.className = "dl";
    dl.textContent = "Baixar";
    dl.hidden = !f.blob;
    dl.addEventListener("click", () => downloadBlob(f.blob, f.outName || f.name));
    const drop = document.createElement("button");
    drop.type = "button";
    drop.className = "dl quiet";
    drop.textContent = "Tirar";
    drop.addEventListener("click", () => {
      state.files = state.files.filter((x) => x.id !== f.id);
      if (state.selected === f.id) state.selected = state.files[0] ? state.files[0].id : null;
      render();
      const sel = state.files.find((x) => x.id === state.selected);
      if (!sel) showBlank();
    });
    li.append(btn, dl, drop);
    jobs.append(li);
  }
  const sel = state.files.find((f) => f.id === state.selected);
  if (sel && (sel.html || sel.text)) showProof(sel);
}

function showProof(f) {
  $("proofName").textContent = f.name;
  const sheet = $("sheet");
  sheet.classList.add("filled");
  sheet.innerHTML = f.html || "<p></p>";
  $("raw").textContent = f.text || "";
  const warns = $("warns");
  if (f.warnings && f.warnings.length) {
    warns.hidden = false;
    warns.textContent = f.warnings.join(" ");
  } else {
    warns.hidden = true;
    warns.textContent = "";
  }
  applyView();
}

function applyView() {
  const page = state.view === "page";
  $("viewPage").setAttribute("aria-pressed", page ? "true" : "false");
  $("viewText").setAttribute("aria-pressed", page ? "false" : "true");
  if (!$("sheet").classList.contains("filled")) {
    $("sheet").hidden = false;
    $("raw").hidden = true;
    return;
  }
  $("sheet").hidden = !page;
  $("raw").hidden = page;
}

async function preview(id) {
  const f = state.files.find((x) => x.id === id);
  if (!f) return;
  f.status = "lendo";
  f.kind = "wait";
  render();
  say("Lendo " + f.name);
  try {
    const body = new FormData();
    body.append("file", f.file, f.name);
    body.append("to", f.to);
    const res = await fetch("/api/preview", { method: "POST", body });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || "falha na leitura");
    f.from = data.from || f.from;
    f.html = data.html || "";
    f.text = data.text || "";
    f.warnings = data.warnings || [];
    f.status = "na fila";
    f.kind = "wait";
    f.error = "";
    if (state.selected === id) showProof(f);
    render();
  } catch (err) {
    f.status = "erro";
    f.kind = "bad";
    f.error = err.message;
    if (state.selected === id) showBlank(err.message);
    render();
    say(err.message);
  }
}

async function convertAll() {
  const btn = $("convert");
  if (!state.files.length) return;
  btn.disabled = true;
  btn.dataset.busy = "1";
  btn.textContent = "Convertendo";
  for (const f of state.files) {
    if (f.blob && f.doneTo === f.to) continue;
    f.status = "convertendo";
    f.kind = "wait";
    f.error = "";
    render();
    say("Convertendo " + f.name + " para " + label(f.to));
    try {
      const body = new FormData();
      body.append("file", f.file, f.name);
      body.append("to", f.to);
      const res = await fetch("/api/convert", { method: "POST", body });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error(data.error || "a conversão falhou");
      }
      const blob = await res.blob();
      const disp = res.headers.get("Content-Disposition") || "";
      const star = /filename\*=UTF-8''([^;]+)/i.exec(disp);
      const plain = /filename="([^"]+)"/.exec(disp);
      f.outName = star ? decodeURIComponent(star[1]) : plain ? plain[1] : f.name;
      f.blob = blob;
      f.doneTo = f.to;
      f.status = "pronto";
      f.kind = "ok";
      const warn = res.headers.get("X-Convert-Warn");
      if (warn) f.warnings = decodeURIComponent(warn).split("\n").filter(Boolean);
    } catch (err) {
      f.status = "erro";
      f.kind = "bad";
      f.error = err.message;
      say(err.message);
    }
    render();
  }
  btn.dataset.busy = "";
  btn.textContent = "Converter";
  btn.disabled = state.files.length === 0;
  const ok = state.files.filter((f) => f.blob).length;
  say(ok ? ok + " arquivo(s) pronto(s) para baixar." : "Nada foi convertido.");
}

function downloadBlob(blob, name) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1500);
}

async function downloadZip() {
  const ready = state.files.filter((f) => f.blob);
  if (ready.length < 2) return;
  const body = new FormData();
  for (const f of ready) {
    body.append("file", f.blob, f.outName || "arquivo");
  }
  const res = await fetch("/api/zip", { method: "POST", body });
  if (!res.ok) {
    say("Não consegui montar o zip.");
    return;
  }
  downloadBlob(await res.blob(), "convertetudo.zip");
}

function renderPalette() {
  const q = ($("q").value || "").trim().toLowerCase();
  const box = $("families");
  box.innerHTML = "";
  const writes = state.formats.filter((f) => f.write && (
    !q || f.label.toLowerCase().includes(q) || f.id.includes(q) || f.family.toLowerCase().includes(q) || f.note.toLowerCase().includes(q)
  ));
  let fam = "";
  let idx = 0;
  for (const f of writes) {
    if (f.family !== fam) {
      fam = f.family;
      const h = document.createElement("div");
      h.className = "fam";
      h.textContent = fam;
      box.appendChild(h);
    }
    const b = document.createElement("button");
    b.type = "button";
    b.className = "opt" + (f.id === state.to ? " on" : "");
    b.dataset.id = f.id;
    b.dataset.i = String(idx++);
    b.innerHTML = "<b></b><span></span>";
    b.querySelector("b").textContent = f.label;
    b.querySelector("span").textContent = f.note;
    b.addEventListener("click", () => choose(f.id));
    box.appendChild(b);
  }
  if (!writes.length) {
    const p = document.createElement("p");
    p.className = "fam";
    p.textContent = "Nenhum formato com esse nome.";
    box.appendChild(p);
  }
}

function openPalette() {
  $("palette").hidden = false;
  $("formatBtn").setAttribute("aria-expanded", "true");
  $("q").value = "";
  renderPalette();
  $("q").focus();
}
function closePalette() {
  $("palette").hidden = true;
  $("formatBtn").setAttribute("aria-expanded", "false");
}
function choose(id) {
  state.to = id;
  for (const f of state.files) {
    if (!f.blob) f.to = id;
  }
  closePalette();
  render();
  const sel = state.files.find((f) => f.id === state.selected);
  if (sel && !sel.blob) preview(sel.id);
}

stage.addEventListener("dragover", (e) => {
  e.preventDefault();
  stage.classList.add("hot");
});
stage.addEventListener("dragleave", (e) => {
  if (!stage.contains(e.relatedTarget)) stage.classList.remove("hot");
});
stage.addEventListener("drop", (e) => {
  e.preventDefault();
  stage.classList.remove("hot");
  addFiles(e.dataTransfer.files);
});
$("pick").addEventListener("click", () => $("file").click());
$("add").addEventListener("click", () => $("file").click());
$("file").addEventListener("change", () => {
  addFiles($("file").files);
  $("file").value = "";
});
$("convert").addEventListener("click", convertAll);
$("zip").addEventListener("click", downloadZip);
$("formatBtn").addEventListener("click", () => {
  if ($("palette").hidden) openPalette();
  else closePalette();
});
$("q").addEventListener("input", renderPalette);
$("viewPage").addEventListener("click", () => { state.view = "page"; applyView(); });
$("viewText").addEventListener("click", () => { state.view = "text"; applyView(); });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closePalette();
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    if (state.files.length) convertAll();
  }
  if (e.key === "/" && document.activeElement !== $("q") && document.activeElement.tagName !== "INPUT") {
    e.preventDefault();
    openPalette();
  }
});
document.addEventListener("click", (e) => {
  if ($("palette").hidden) return;
  if (e.target.closest("#palette") || e.target.closest("#formatBtn")) return;
  closePalette();
});

boot().catch((err) => say(err.message));
