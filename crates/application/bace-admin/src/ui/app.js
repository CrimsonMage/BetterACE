"use strict";
const byId = id => document.getElementById(id);
let csrf = "", events = null, timer = null;
const message = text => { byId("message").textContent = text; };
async function request(path, body) {
  const response = await fetch(path, {method: body ? "POST" : "GET", headers: body ? {"Content-Type":"application/json", "X-CSRF-Token":csrf} : {}, body: body ? JSON.stringify(body) : undefined});
  if (!response.ok) throw new Error(`Request failed (${response.status}).`);
  return response.status === 204 ? {} : response.json();
}
function signedOut() { csrf=""; if(events) events.close(); clearInterval(timer); byId("login").hidden=false; byId("dashboard").hidden=true; byId("logout").hidden=true; }
async function status() {
  try {
    const state = await request("/api/status");
    byId("phase").textContent=state.phase;
    byId("generation").textContent=`${state.generation} / ${state.child_pid ?? "none"}`;
    byId("operation").textContent=state.operation_id ?? "none";
    byId("detail").textContent=state.detail;
    byId("restart").disabled=["starting","draining","stopping","blocked"].includes(state.phase);
    byId("retry").disabled=state.phase!=="blocked";
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
request("/api/session").then(result=>signedIn(result.csrf)).catch(()=>signedOut());
