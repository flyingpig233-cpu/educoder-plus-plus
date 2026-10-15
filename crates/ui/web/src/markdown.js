// Repair Educoder's missing ATX-heading spaces without altering code examples.
export function normalizeTaskMarkdown(source = '') {
  let fence = null, htmlCode = false;
  return source.split('\n').map(line => {
    const body=line.replace(/^ {0,3}(?:>\s*)*/, '');
    const marker=body.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
    if(fence){if(marker&&marker[1][0]===fence.char&&marker[1].length>=fence.length&&!marker[2].trim())fence=null;return line;}
    if(/<(?:pre|code)\b/i.test(body))htmlCode=true;
    if(htmlCode){if(/<\/(?:pre|code)>/i.test(body))htmlCode=false;return line;}
    if(marker){fence={char:marker[1][0],length:marker[1].length};return line;}
    if(/^\s*#(?:include|define|ifdef|ifndef|if|else|elif|endif|pragma|undef|error|line)\b/.test(body))return line;
    return line.replace(/^( {0,3}(?:>\s*)*)(#{1,6})(?=[^#\s])/, '$1$2 ');
  }).join('\n');
}
