//! Independent desktop language. English source keys stay stable in workers and
//! on disk; translation happens at the presentation boundary, including messages
//! already on screen when the language changes. Never translate user filenames
//! or layer names. The CLI and PhotoCraft action identifiers remain unchanged.
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}
impl Default for Language {
    fn default() -> Self {
        Self::from_locale(&sys_locale::get_locale().unwrap_or_default())
    }
}
impl Language {
    pub const ALL: [Self; 2] = [Self::Chinese, Self::English];
    pub fn from_locale(locale: &str) -> Self {
        if locale
            .split(['-', '_', '.'])
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case("zh"))
        {
            Self::Chinese
        } else {
            Self::English
        }
    }
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "zh-CN" => Some(Self::Chinese),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "简体中文",
        }
    }
    pub fn text<'a>(self, source: &'a str) -> Cow<'a, str> {
        if self == Self::English || source.is_empty() {
            return Cow::Borrowed(source);
        }
        if let Some((_, translated)) = catalog().find(|(key, _)| *key == source) {
            return Cow::Borrowed(translated);
        }
        for (key, translated) in catalog().filter(|(key, _)| key.contains('{')) {
            if let Some(values) = capture(key, source) {
                return Cow::Owned(self.render(translated, values));
            }
        }
        Cow::Borrowed(source)
    }
    /// Third-party / OS diagnostics have no fixed vocabulary. Show a localized
    /// recovery message and retain the original separately in Technical details.
    pub fn error<'a>(self, source: &'a str) -> Cow<'a, str> {
        if self == Self::English {
            if catalog().any(|(key, _)| key == source) {
                return Cow::Borrowed(source);
            }
            for (key, _) in catalog().filter(|(key, _)| key.contains('{')) {
                if let Some(values) = capture(key, source) {
                    return Cow::Owned(self.render(key, values));
                }
            }
            return self.text("The operation could not be completed. Check the file and access permissions, then try again.");
        }
        let text = self.text(source);
        if self == Self::Chinese && text == source {
            self.text("The operation could not be completed. Check the file and access permissions, then try again.")
        } else {
            text
        }
    }
    fn render(self, template: &str, values: Vec<(&str, &str)>) -> String {
        let mut output = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            output.push_str(&rest[..start]);
            let Some(end) = rest[start..].find('}').map(|end| start + end) else {
                break;
            };
            let name = &rest[start + 1..end];
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
                // Only nested diagnostics are translatable. Append other data
                // once so braces inside a filename cannot become placeholders.
                if name == "error" {
                    output.push_str(&self.error(value));
                } else {
                    output.push_str(value);
                }
            }
            rest = &rest[end + 1..];
        }
        output.push_str(rest);
        output
    }
}
fn catalog() -> impl Iterator<Item = (&'static str, &'static str)> {
    include_str!("locales/zh-CN.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(|line| line.split_once('\t'))
}

/// Match anchored source templates. Placeholders are separated by literal text,
/// so neither arbitrary replacement nor translation of user data is needed.
fn capture<'a, 'b>(template: &'a str, source: &'b str) -> Option<Vec<(&'a str, &'b str)>> {
    let mut rest = source;
    let mut pattern = template;
    let mut values = Vec::new();
    while let Some(start) = pattern.find('{') {
        rest = rest.strip_prefix(&pattern[..start])?;
        pattern = &pattern[start + 1..];
        let end = pattern.find('}')?;
        let name = &pattern[..end];
        pattern = &pattern[end + 1..];
        let literal = pattern.split('{').next()?;
        let length = if literal.is_empty() {
            rest.len()
        } else {
            rest.find(literal)?
        };
        values.push((name, &rest[..length]));
        rest = &rest[length..];
    }
    (rest == pattern).then_some(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_unique_and_preserves_template_placeholders() {
        let mut keys = std::collections::HashSet::new();
        for (key, value) in catalog() {
            assert!(keys.insert(key), "Duplicate translation: {key}");
            assert!(!value.is_empty(), "Empty translation: {key}");
            let placeholders = |s: &str| {
                s.split('{')
                    .skip(1)
                    .map(|p| p.split('}').next().unwrap().to_owned())
                    .collect::<std::collections::BTreeSet<_>>()
            };
            assert_eq!(placeholders(key), placeholders(value), "{key}");
        }
    }
    #[test]
    fn locale_matching_and_explicit_codes() {
        for tag in ["zh_CN.UTF-8", "zh-Hans-CN", "zh-TW", "ZH-cn"] {
            assert_eq!(Language::from_locale(tag), Language::Chinese);
        }
        assert_eq!(Language::from_locale("en-US"), Language::English);
        assert_eq!(Language::from_locale(""), Language::English);
        assert_eq!(Language::from_code("unsupported"), None);
    }
    #[test]
    fn messages_retranslate_without_changing_paths_or_numbers() {
        let en = Language::English;
        let zh = Language::Chinese;
        assert_eq!(en.text("Preview complete"), "Preview complete");
        assert_eq!(zh.text("Preview complete"), "预览已更新");
        assert_eq!(zh.text("12 ms · cached filter"), "12 毫秒 · 已复用滤镜缓存");
        assert_eq!(zh.text("Saved: 我的照片.pcraft"), "已保存：我的照片.pcraft");
        assert_eq!(zh.text("Could not update the connection: Choose a PhotoCraft .app bundle. Choose PhotoCraft again, then save the connection."), "无法更新连接：请选择 PhotoCraft 的 .app 应用。请重新选择 PhotoCraft，然后保存连接。");
        assert_eq!(
            zh.text("ICC RGB ↔ sRGB · source profile retained"),
            "ICC RGB ↔ sRGB · 保留源色彩配置"
        );
        assert!(
            zh.error("unrecognized third-party diagnostic")
                .contains("请检查")
        );
        assert!(
            en.error("未知的系统诊断")
                .starts_with("The operation could not be completed.")
        );
        assert_eq!(zh.text("Saved: {error}.pcraft"), "已保存：{error}.pcraft");
    }
}
