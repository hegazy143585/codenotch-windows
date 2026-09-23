/* Provider-note dictionary (W-23, W-26), shared by the card (notch.html merges it into STR) and the
   settings window. Keys are the note codes of src/notes.rs and en must equal its templates; ar is
   complete. zh/ja/ko fall back to English. The i18n tests in src/i18n.rs check all of this. */
const NOTE_STR={
  en:{
    nRateLimited:'Rate limited — retrying in {0}s',nOffline:'Offline — {0}',nLiveFailed:'Live read failed ({0})',nVia:'via {0}',
    nNoWindows:'{0} reported no usage windows',nNothingMetered:'{0} has nothing metered on this account yet',nUnlimitedQuotas:'{0} reported no metered quotas (unlimited)',nCredRefreshed:'Credential refreshed — fetching',
    nClaudeNoCred:'No Claude Code credential found',nClaudeExpired:'Credential expired — run any claude command (or chat with Claude) to refresh it',nClaudeExpiredLimited:'Credential expired — run any claude command (or chat with Claude) to refresh it; the old token is rate-limited until then',nClaudeRejected:'Credential rejected (switched accounts?)',
    nViaDesktop:'Live via Claude Desktop (it samples every 15 min)',nCodexExpired:'Codex sign-in expired — open Codex once to refresh it',nCodexRejected:'Codex rejected its sign-in — sign in to Codex again',nFromLastRun:'from last Codex run',
    nCodexNoSnapshot:'Codex has not recorded a usage snapshot yet',nCursorSignIn:'Sign in to Cursor (the editor) to see usage.',nCursorRejected:'Cursor session was rejected — sign in again in the editor',nCursorUnlimited:'Unlimited on the {0} plan — nothing to meter',
    nCursorNothing:'The {0} plan has nothing for Cursor to meter yet',nCursorUnlimitedAny:'Unlimited plan — nothing to meter',nCursorNothingAny:'This plan has nothing for Cursor to meter yet',nAgClosed:'Antigravity is closed — last reading kept',
    nAgRejected:'Antigravity’s Google session was rejected — sign in again in Antigravity',nAgNoQuota:'Google publishes no quota for this account',nAgOpen:'Open Antigravity to read its quota',nGeminiCalls:'{0} calls this month',
    nPerToken:'billed per token, no limit',nLocalLogs:'counted from local logs; your API key is never read',nOllamaLoaded:'Loaded: {0}',nOllamaNoModel:'Server running · no model loaded',
    nLocalNoQuota:'local server, no quota',nOllamaDown:'Ollama server not running — open Ollama to see loaded models',nOllamaNoUsage:'No Ollama cloud usage recorded yet for this period',nNoResetDate:'the API publishes no reset date',
    nOllamaKeyRejected:'Ollama rejected the API key — create a new one at ollama.com/settings/keys',nCmdRejected:'Command Code rejected the key — sign in again in the Command Code app',nGhRejected:'GitHub rejected the token — run `gh auth login` and make sure Copilot is enabled',nZaiRejected:'Z.ai rejected the plan key — sign in again in the tool that holds it',
    nGrokExpired:'Grok sign-in expired — run `grok login` to refresh it',nGrokRejected:'Grok rejected the sign-in — run `grok login`',nOpencodeNoSub:'No OpenCode Go subscription on this key',nOpencodeRejected:'OpenCode rejected the Go key — run `opencode auth login` again',
    nPplxCounts:'counts left; no totals or reset times are published',nPplxSignIn:'Sign in, or pass Perplexity’s check: click the Perplexity cell'},
  ar:{
    nRateLimited:'تم تجاوز حد الطلبات — إعادة المحاولة بعد {0} ث',nOffline:'غير متصل — {0}',nLiveFailed:'تعذّرت القراءة المباشرة ({0})',nVia:'عبر {0}',
    nNoWindows:'لم يُبلغ {0} عن فترات استخدام',nNothingMetered:'لا شيء يُقاس في {0} على هذا الحساب بعد',nUnlimitedQuotas:'لم يُبلغ {0} عن حصص مقيسة (غير محدود)',nCredRefreshed:'تم تحديث بيانات الاعتماد — جارٍ الجلب',
    nClaudeNoCred:'لم يُعثر على بيانات اعتماد Claude Code',nClaudeExpired:'انتهت صلاحية بيانات الاعتماد — شغّل أي أمر claude (أو تحدّث مع Claude) لتحديثها',nClaudeExpiredLimited:'انتهت صلاحية بيانات الاعتماد — شغّل أي أمر claude (أو تحدّث مع Claude) لتحديثها؛ الرمز القديم مقيّد بحد الطلبات حتى ذلك الحين',nClaudeRejected:'رُفضت بيانات الاعتماد (هل بدّلت الحساب؟)',
    nViaDesktop:'مباشر عبر Claude Desktop (يأخذ قراءة كل 15 دقيقة)',nCodexExpired:'انتهت صلاحية تسجيل الدخول إلى Codex — افتح Codex مرة لتحديثه',nCodexRejected:'رفض Codex تسجيل الدخول — سجّل الدخول إلى Codex مجددًا',nFromLastRun:'من آخر تشغيل لـ Codex',
    nCodexNoSnapshot:'لم يسجّل Codex لقطة استخدام بعد',nCursorSignIn:'سجّل الدخول إلى Cursor (المحرر) لرؤية الاستخدام.',nCursorRejected:'رُفضت جلسة Cursor — سجّل الدخول مجددًا في المحرر',nCursorUnlimited:'خطة {0} غير محدودة — لا شيء يُقاس',
    nCursorNothing:'لا يوجد في خطة {0} ما يقيسه Cursor بعد',nCursorUnlimitedAny:'خطة غير محدودة — لا شيء يُقاس',nCursorNothingAny:'لا يوجد في هذه الخطة ما يقيسه Cursor بعد',nAgClosed:'Antigravity مغلق — أُبقيت آخر قراءة',
    nAgRejected:'رُفضت جلسة Google في Antigravity — سجّل الدخول مجددًا في Antigravity',nAgNoQuota:'لا تنشر Google حصة لهذا الحساب',nAgOpen:'افتح Antigravity لقراءة حصته',nGeminiCalls:'{0} طلب هذا الشهر',
    nPerToken:'يُحتسب لكل رمز، بلا حد',nLocalLogs:'محسوب من السجلات المحلية؛ لا يُقرأ مفتاح API الخاص بك أبدًا',nOllamaLoaded:'محمّل: {0}',nOllamaNoModel:'الخادم يعمل · لا يوجد نموذج محمّل',
    nLocalNoQuota:'خادم محلي، بلا حصة',nOllamaDown:'خادم Ollama لا يعمل — افتح Ollama لرؤية النماذج المحمّلة',nOllamaNoUsage:'لا يوجد استخدام سحابي مسجّل في Ollama لهذه الفترة بعد',nNoResetDate:'لا تنشر الواجهة البرمجية موعد إعادة الضبط',
    nOllamaKeyRejected:'رفض Ollama مفتاح API — أنشئ مفتاحًا جديدًا من ollama.com/settings/keys',nCmdRejected:'رفض Command Code المفتاح — سجّل الدخول مجددًا في تطبيق Command Code',nGhRejected:'رفض GitHub الرمز — شغّل `gh auth login` وتأكد من تفعيل Copilot',nZaiRejected:'رفض Z.ai مفتاح الخطة — سجّل الدخول مجددًا في الأداة التي تحمله',
    nGrokExpired:'انتهت صلاحية تسجيل الدخول إلى Grok — شغّل `grok login` لتحديثه',nGrokRejected:'رفض Grok تسجيل الدخول — شغّل `grok login`',nOpencodeNoSub:'لا يوجد اشتراك OpenCode Go على هذا المفتاح',nOpencodeRejected:'رفض OpenCode مفتاح Go — شغّل `opencode auth login` مجددًا',
    nPplxCounts:'الأعداد المتبقية؛ لا تُنشر المجاميع ولا مواعيد إعادة الضبط',nPplxSignIn:'سجّل الدخول، أو اجتز فحص Perplexity: انقر خلية Perplexity'},
};
/* Parts [{code,args}] as text in `lang` (English for a code the language lacks); null when a code is
   unknown. `text` parts are names or raw details shown as they are. Arguments are filled in one pass. */
function notePartsText(parts,lang,dict){
  const d=dict||NOTE_STR, out=[];
  for(const pt of parts){
    let s;
    if(pt.code==='text') s=(pt.args||[])[0]||'';
    else{
      const tpl=(d[lang]&&d[lang][pt.code])??(d.en&&d.en[pt.code]);
      if(tpl==null) return null;
      s=tpl.replace(/\{(\d+)\}/g,(_,i)=>(pt.args||[])[+i]??'');
    }
    if(s) out.push(s);
  }
  return out.join(' · ');
}
/* The note to show. The parts are used only when their English rendering equals `note` (the same
   templates as src/notes.rs); otherwise, e.g. a reading persisted before the codes existed or a code
   this page does not know, `note` is shown as it is. */
function localNote(note,parts,lang,dict){
  if(!parts||!parts.length||notePartsText(parts,'en',dict)!==note) return note||'';
  return notePartsText(parts,lang,dict);
}
