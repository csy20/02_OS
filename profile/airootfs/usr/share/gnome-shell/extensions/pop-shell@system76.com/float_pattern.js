// User-selected titles and classes are literals. Built-in rules stay regular
// expressions. A stored pattern that does not compile is matched literally so
// one bad rule cannot throw out of classification.

export function escape_regex_literal(value) {
    return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function literal_rule_field(value) {
    if (typeof value !== 'string')
        return value;
    return escape_regex_literal(value);
}

export function regex_matches(pattern, text) {
    const subject = String(text);
    const compile = (source) => new RegExp(source, 'i').test(subject);
    try {
        return compile(pattern);
    }
    catch (_invalid) {
        try {
            return compile(escape_regex_literal(pattern));
        }
        catch (_still_invalid) {
            return false;
        }
    }
}

export function exception_applies(rules, wclass, title) {
    for (const rule of rules) {
        if (rule.class && !regex_matches(rule.class, wclass))
            continue;
        if (rule.title && !regex_matches(rule.title, title))
            continue;
        return rule.disabled ? false : true;
    }
    return false;
}
