//! The server's web page: download the desktop client, set the first password.

use super::*;

const EXE: &str = "rutable.exe";

fn exe(state: &App) -> Option<(std::path::PathBuf, std::fs::Metadata)> {
    let path = state.dist.as_ref()?.join(EXE);
    let meta = std::fs::metadata(&path).ok()?;
    meta.is_file().then_some((path, meta))
}

pub async fn download(State(state): State<App>) -> Result<Response> {
    let (path, _) = exe(&state).ok_or_else(|| error(StatusCode::NOT_FOUND, "客户端尚未发布"))?;
    let body = tokio::fs::read(path).await.map_err(|_| error(StatusCode::NOT_FOUND, "客户端尚未发布"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.microsoft.portable-executable".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{EXE}\"")),
        ],
        body,
    )
        .into_response())
}

pub async fn home(State(state): State<App>) -> Html<String> {
    let file = exe(&state).map(|(_, m)| {
        let mb = m.len() as f64 / 1_048_576.0;
        let when = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| rutable_model::value::format_date(d.as_millis() as i64, rutable_model::DateFormat::YmdDash, true))
            .unwrap_or_default();
        format!("Windows 10/11 · {mb:.1} MB · 更新于 {}", escape(&when))
    });
    let button = match file {
        Some(meta) => format!(r#"<a class="btn" href="/download/{EXE}" download>下载 Windows 客户端</a><p class="meta">{meta}</p>"#),
        None => r#"<span class="btn off">客户端尚未发布</span>"#.to_string(),
    };
    Html(PAGE.replace("{{BUTTON}}", &button))
}

const PAGE: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="referrer" content="no-referrer"><title>Rutable 多维表格</title>
<style>
:root{--bg:#f5f6f7;--card:#fff;--text:#1f2329;--muted:#646a73;--line:#dee0e3;--primary:#3370ff;--primary-hover:#245bdb;--ok:#2ea121;--bad:#f54a45;--field:#fff}
@media (prefers-color-scheme:dark){:root{--bg:#17181a;--card:#222326;--text:#ebebeb;--muted:#a6a6a6;--line:#3a3b3e;--primary:#4c83ff;--primary-hover:#6b98ff;--field:#1b1c1e}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:15px/1.6 -apple-system,"Segoe UI","PingFang SC","Microsoft YaHei",sans-serif}
main{max-width:880px;margin:0 auto;padding:56px 16px 64px}
.logo{display:flex;align-items:center;gap:12px;font-size:28px;font-weight:650}
.logo i{display:grid;place-items:center;width:44px;height:44px;border-radius:10px;background:var(--primary);color:#fff;font-style:normal;font-size:24px}
.lead{color:var(--muted);font-size:17px;margin:12px 0 28px}
.btn{display:inline-block;background:var(--primary);color:#fff;text-decoration:none;padding:12px 22px;border-radius:8px;font-weight:600;border:0;font-size:15px;cursor:pointer}
.btn:hover{background:var(--primary-hover)}.btn.off{background:var(--line);color:var(--muted);cursor:default}
.meta{color:var(--muted);font-size:13px;margin:8px 0 0}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:16px;margin-top:40px}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:20px}
.card h3{margin:0 0 6px;font-size:16px}.card p{margin:0;color:var(--muted);font-size:14px}
.step{display:inline-grid;place-items:center;width:24px;height:24px;border-radius:50%;background:var(--primary);color:#fff;font-size:13px;margin-right:8px}
#setup{margin-top:40px}label{display:block;font-size:13px;color:var(--muted);margin:12px 0 4px}
input{width:100%;max-width:420px;padding:10px 12px;border:1px solid var(--line);border-radius:8px;background:var(--field);color:var(--text);font-size:15px}
input:focus{outline:2px solid var(--primary);border-color:transparent}
.msg{margin-top:12px;font-size:14px}.msg.ok{color:var(--ok)}.msg.bad{color:var(--bad)}
code{background:var(--bg);border:1px solid var(--line);border-radius:4px;padding:1px 5px;font-size:13px}
</style></head><body><main>
<div class="logo"><i>▦</i>Rutable 多维表格</div>
<p class="lead">在桌面上管理你的多维表格，数据保存在你自己的服务器。AI 可以通过 MCP 直接读写表格，客户端实时显示变化。</p>
{{BUTTON}}
<div class="grid">
 <div class="card"><h3><span class="step">1</span>下载并打开客户端</h3><p>双击 rutable.exe 即可运行，无需安装。</p></div>
 <div class="card"><h3><span class="step">2</span>设置或输入访问密码</h3><p>首次使用时设置密码；之后每台电脑登录一次即可，客户端会记住登录。</p></div>
 <div class="card"><h3><span class="step">3</span>把表格交给 AI</h3><p>在客户端点击「复制给 AI」，粘贴到 Claude Code、Cursor 等支持 MCP 的工具里。</p></div>
</div>
<section id="setup" class="card" hidden>
 <h3>设置访问密码</h3>
 <p>这台服务器还没有设置密码。为防止他人抢先设置，需要输入服务器密钥（部署时的 <code>RUTABLE_API_TOKEN</code>）。</p>
 <form id="f"><label for="k">服务器密钥</label><input id="k" type="password" autocomplete="off" required>
 <label for="p">新密码（至少 8 个字符）</label><input id="p" type="password" autocomplete="new-password" minlength="8" required>
 <label for="p2">再次输入</label><input id="p2" type="password" autocomplete="new-password" minlength="8" required>
 <p><button class="btn" type="submit" style="margin-top:16px">设置密码</button></p><div id="m" class="msg" role="status"></div></form>
</section>
<p id="done" class="meta" hidden>✓ 已设置访问密码，打开客户端用密码登录即可。</p>
</main><script>
const $=id=>document.getElementById(id);
fetch('/api/v1/auth/status').then(r=>r.json()).then(s=>{ if(s.password_set){$('done').hidden=false}else{$('setup').hidden=false} }).catch(()=>{});
$('f').addEventListener('submit',async e=>{e.preventDefault();const m=$('m');m.className='msg';
 if($('p').value!==$('p2').value){m.className='msg bad';m.textContent='两次输入的密码不一致';return}
 m.textContent='正在设置…';
 try{const r=await fetch('/api/v1/auth/setup',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({setup_key:$('k').value,password:$('p').value,device:'网页设置'})});
  const b=await r.json().catch(()=>({}));
  if(r.ok){m.className='msg ok';m.textContent='设置完成。打开客户端，用这个密码登录。';$('f').querySelectorAll('input,button').forEach(x=>x.disabled=true)}
  else{m.className='msg bad';m.textContent=b.error||('设置失败（'+r.status+'）')}}catch(x){m.className='msg bad';m.textContent='无法连接服务器'}});
</script></body></html>"#;
