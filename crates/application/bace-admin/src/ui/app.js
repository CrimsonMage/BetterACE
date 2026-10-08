"use strict";
const byId = id => document.getElementById(id);
let csrf = "", events = null, timer = null;
let contentToken = "", contentTotal = 0;
let queuedRevision = 0;
const message = text => { byId("message").textContent = text; };
async function request(path, body) {
  const response = await fetch(path, {method: body ? "POST" : "GET", headers: body ? {"Content-Type":"application/json", "X-CSRF-Token":csrf} : {}, body: body ? JSON.stringify(body) : undefined});
  const raw = response.status === 204 ? "" : await response.text();
  let result = {};
  if(raw) { try { result = JSON.parse(raw); } catch { if(response.ok) throw new Error("Server returned an invalid response."); } }
  if (!response.ok) throw new Error(result.error || `Request failed (${response.status}).`);
  return result;
}
function signedOut() { csrf=""; contentToken=""; queuedRevision=0; if(events) events.close(); clearInterval(timer); byId("login").hidden=false; byId("dashboard").hidden=true; byId("logout").hidden=true; }
async function status() {
  try {
    const state = await request("/api/status");
    byId("phase").textContent=state.phase;
    byId("generation").textContent=`${state.generation} / ${state.child_pid ?? "none"}`;
    byId("operation").textContent=state.operation_id ?? "none";
    byId("detail").textContent=state.detail;
    byId("restart").disabled=["starting","draining","stopping","blocked"].includes(state.phase);
    byId("retry").disabled=state.phase!=="blocked";
    try {
      const content=await request("/api/content/status");
      byId("content-status").textContent=`Accepted revision ${content.accepted_revision} · pack generation ${content.pack_generation ?? "none"} · pending ${content.pending_publications} · rejected ${content.rejected_publications}`;
      if(queuedRevision) {
        const decision=await request(`/api/content/revision?revision=${queuedRevision}`);
        if(decision.status==="accepted") {
          byId("content-summary").textContent=`Revision ${queuedRevision} accepted into the immutable pack.`;
          queuedRevision=0;
        } else if(decision.status==="rejected") {
          byId("content-summary").textContent=`Revision ${queuedRevision} rejected: ${decision.rejection || "inspect host diagnostics"}`;
          queuedRevision=0;
        } else {
          byId("content-summary").textContent=`Revision ${queuedRevision} is awaiting pack validation and publication.`;
        }
      }
    } catch(error) { byId("content-status").textContent=error.message; }
  } catch(error) { message(error.message); }
}
function signedIn(token) {
  csrf=token; byId("login").hidden=true; byId("dashboard").hidden=false; byId("logout").hidden=false; message("");
  status(); timer=setInterval(status,2000);
  events=new EventSource("/api/logs");
  events.addEventListener("logs", event => {
    const batch=JSON.parse(event.data), rows=byId("logs").textContent.split("\n");
    if(batch.gap) rows.push("[Some older diagnostics have expired.]");
    for(const record of batch.records) rows.push(`${new Date(record.unix_millis).toISOString()} ${record.level.toUpperCase()} ${record.event}: ${record.message}`);
    byId("logs").textContent=rows.slice(-500).join("\n");
    byId("log-health").textContent=`Live · dropped: ${batch.dropped} · disk: ${batch.disk_degraded ? "degraded" : "available"}`;
  });
  events.onerror=()=>{ byId("log-health").textContent="Stream disconnected; reconnecting or sign in again."; };
}
byId("login-form").addEventListener("submit", async event => {
  event.preventDefault(); const password=byId("password").value; byId("password").value="";
  try { signedIn((await request("/api/login", {password})).csrf); } catch(error) { message(error.message); }
});
byId("logout").onclick=async()=>{ try { await request("/api/logout", {}); signedOut(); } catch(error) { message(error.message); } };
for(const action of ["restart","retry"]) byId(action).onclick=async()=>{
  try { const result=await request("/api/control",{action}); message(`Operation ${result.operation_id} accepted. Completion is shown in worker status.`); await status(); } catch(error) { message(error.message); }
};
const removalKinds = ["weenie","landblock_instance","encounter","landblock_instance_link","quest","recipe","recipe_mod","recipe_mods_bool","recipe_mods_d_i_d","recipe_mods_float","recipe_mods_i_i_d","recipe_mods_int","recipe_mods_string","recipe_requirements_bool","recipe_requirements_d_i_d","recipe_requirements_float","recipe_requirements_i_i_d","recipe_requirements_int","recipe_requirements_string","clothing","loot","rare","cook_book","event","house_portal","points_of_interest","spell","treasure_death","treasure_gem_count","treasure_material_base","treasure_material_color","treasure_material_groups","treasure_wielded","version"];
for(const kind of removalKinds) {
  const option=document.createElement("option"); option.value=kind; option.textContent=kind.replaceAll("_"," "); byId("content-remove-kind").append(option);
}
byId("content-remove-form").addEventListener("submit", async event=>{
  event.preventDefault();
  const kind=byId("content-remove-kind").value, text=byId("content-remove-id").value.trim();
  if(!/^(0x[0-9a-f]+|[0-9]+)$/i.test(text)) { message("Enter a decimal or hexadecimal record ID."); return; }
  const id=Number(text);
  if(!Number.isSafeInteger(id) || id<0 || id>4294967295) { message("Record ID is outside the supported range."); return; }
  try {
    const result=await request("/api/content/removal", {kind,id});
    message(`Removal staged in ${result.path}. Review the inbox before publishing.`);
    byId("content-preview").click();
  } catch(error) { message(error.message); }
});
byId("content-preview").onclick=async()=>{
  contentToken=""; byId("content-publish").hidden=true; byId("content-list").replaceChildren();
  try {
    const preview=await request("/api/content/preview", {});
    contentToken=preview.token; contentTotal=preview.total;
    byId("content-summary").textContent=`${preview.total} change${preview.total===1?"":"s"} found. Review every entry before queueing.`;
    const list=document.createElement("ul");
    for(const entry of preview.entries) {
      const row=document.createElement("li");
      row.textContent=`${entry.action.toUpperCase()} · ${entry.kind} ${entry.id} · ${entry.name} · ${entry.path}`;
      list.append(row);
    }
    byId("content-list").append(list);
    byId("content-publish").hidden=preview.total===0;
  } catch(error) { byId("content-summary").textContent=error.message; }
};
byId("content-publish").onclick=async()=>{
  if(!contentToken || !window.confirm(`Queue these ${contentTotal} reviewed content changes? Existing matching entries will be replaced and explicit removals will be hidden from new lookups.`)) return;
  const token=contentToken; contentToken=""; byId("content-publish").hidden=true;
  try {
    const result=await request("/api/content/publish", {token});
    queuedRevision=result.revision;
    byId("content-summary").textContent=`Queued durable candidate revision ${result.revision}. Activation is reported separately by the content worker.`;
  } catch(error) { byId("content-summary").textContent=error.message; }
};
request("/api/session").then(result=>signedIn(result.csrf)).catch(()=>signedOut());
