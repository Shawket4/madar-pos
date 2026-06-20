//! Static UI-string localization (PLAN: shared core owns logic). One source of
//! truth for both hosts — a string change here lands in Swift AND Kotlin at once.
//! Dynamic content (menu `*_translations`) is resolved separately in `menu`.
//!
//! Resolution: device locale → its language subtag → `en` → the key itself.
//! RTL languages (ar/…) are flagged so the host can flip layout direction.

/// Localized string for `key` in `locale`, falling back en → key.
pub fn tr(locale: &str, key: &str) -> String {
    let lang = lang_of(locale);
    let resolved = match lang {
        "ar" => ar(key),
        _ => None,
    };
    resolved.or_else(|| en(key)).unwrap_or(key).to_string()
}

pub fn is_rtl(locale: &str) -> bool {
    matches!(lang_of(locale), "ar" | "fa" | "he" | "ur")
}

fn lang_of(locale: &str) -> &str {
    locale.split(['-', '_']).next().unwrap_or(locale)
}

fn en(key: &str) -> Option<&'static str> {
    Some(match key {
        // login (teller)
        "login.welcome_back" => "Welcome back",
        "login.subtitle" => "Sign in to open your till",
        "login.name" => "Name",
        "login.sign_in" => "Sign in",
        "login.pin_hint" => "PIN auto-submits at 6 digits",
        "login.reconfigure" => "Reconfigure device",
        "login.branch" => "Branch",
        // device setup (manager)
        "setup.title" => "Configure this till",
        "setup.desc" => "A manager signs in to bind this device to a branch. Tellers sign in after.",
        "setup.choose_branch" => "Choose a branch",
        "setup.choose_branch_desc" => "Bind this till to one of your branches.",
        "setup.email" => "Manager email",
        "setup.password" => "Password",
        "setup.continue" => "Continue",
        "setup.cancel" => "Cancel",
        // brand panel
        "brand.headline" => "Welcome\nback.",
        "brand.tagline" => "Sign in to open your till. Works online and off — your sales keep flowing either way.",
        // home
        "home.signed_in" => "Signed in",
        "home.online" => "Online",
        "home.offline" => "Offline",
        "home.sign_out" => "Sign out",
        "home.teller" => "teller",
        "home.role" => "role",
        "home.currency" => "currency",
        "home.session" => "session",
        // shift
        "shift.open_title" => "Open your shift",
        "shift.opening_desc" => "Count the cash in the drawer to start selling.",
        "shift.opening_cash" => "Opening cash",
        "shift.open_button" => "Open shift",
        "shift.signed_in_as" => "Signed in as",
        "shift.switch_teller" => "Switch teller",
        // order
        "order.title" => "Order",
        "order.coming_soon" => "Catalog & ordering — coming next.",
        "order.close_shift" => "Close shift",
        "order.all" => "All",
        "order.search" => "Search items",
        "order.empty" => "No items here yet.",
        "order.empty_search" => "Nothing matches your search.",
        "order.cart" => "Cart",
        "order.cart_empty" => "Your cart is empty.",
        // errors (host-side messages)
        "err.offline_no_setup" => "You're offline and this teller hasn't been set up for offline sign-in yet.",
        "err.network" => "Network problem, please try again.",
        "err.not_allowed" => "You don't have permission to do that.",
        "err.generic" => "Something went wrong.",
        _ => return None,
    })
}

fn ar(key: &str) -> Option<&'static str> {
    Some(match key {
        "login.welcome_back" => "مرحبًا بعودتك",
        "login.subtitle" => "سجّل الدخول لفتح الخزينة",
        "login.name" => "الاسم",
        "login.sign_in" => "تسجيل الدخول",
        "login.pin_hint" => "يُرسل الرقم السري تلقائيًا عند ٦ أرقام",
        "login.reconfigure" => "إعادة ضبط الجهاز",
        "login.branch" => "فرع",
        "setup.title" => "إعداد نقطة البيع",
        "setup.desc" => "يسجّل المدير الدخول لربط هذا الجهاز بفرع، ثم يسجّل أمناء الصندوق الدخول.",
        "setup.choose_branch" => "اختر فرعًا",
        "setup.choose_branch_desc" => "اربط نقطة البيع بأحد فروعك.",
        "setup.email" => "بريد المدير",
        "setup.password" => "كلمة المرور",
        "setup.continue" => "متابعة",
        "setup.cancel" => "إلغاء",
        "brand.headline" => "مرحبًا\nبعودتك.",
        "brand.tagline" => "سجّل الدخول لفتح الخزينة. يعمل بالاتصال وبدونه — مبيعاتك مستمرة في الحالتين.",
        "home.signed_in" => "تم تسجيل الدخول",
        "home.online" => "متصل",
        "home.offline" => "غير متصل",
        "home.sign_out" => "تسجيل الخروج",
        "home.teller" => "أمين الصندوق",
        "home.role" => "الدور",
        "home.currency" => "العملة",
        "home.session" => "الجلسة",
        "shift.open_title" => "افتح ورديتك",
        "shift.opening_desc" => "احسب النقد في الدرج لبدء البيع.",
        "shift.opening_cash" => "النقد الافتتاحي",
        "shift.open_button" => "فتح الوردية",
        "shift.signed_in_as" => "مسجّل الدخول باسم",
        "shift.switch_teller" => "تبديل الأمين",
        "order.title" => "طلب",
        "order.coming_soon" => "القائمة والطلبات — قريبًا.",
        "order.close_shift" => "إغلاق الوردية",
        "order.all" => "الكل",
        "order.search" => "ابحث عن صنف",
        "order.empty" => "لا توجد أصناف هنا بعد.",
        "order.empty_search" => "لا شيء يطابق بحثك.",
        "order.cart" => "السلة",
        "order.cart_empty" => "سلتك فارغة.",
        "err.offline_no_setup" => "أنت غير متصل ولم يتم تهيئة هذا الأمين للدخول دون اتصال بعد.",
        "err.network" => "مشكلة في الشبكة، حاول مرة أخرى.",
        "err.not_allowed" => "ليس لديك صلاحية للقيام بذلك.",
        "err.generic" => "حدث خطأ ما.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_locale_then_falls_back() {
        assert_eq!(tr("ar-EG", "login.sign_in"), "تسجيل الدخول");
        assert_eq!(tr("ar", "login.sign_in"), "تسجيل الدخول");
        assert_eq!(tr("en", "login.sign_in"), "Sign in");
        assert_eq!(tr("fr", "login.sign_in"), "Sign in"); // unknown lang → en
        assert_eq!(tr("en", "no.such.key"), "no.such.key"); // unknown key → key
    }

    #[test]
    fn rtl_detection() {
        assert!(is_rtl("ar-EG"));
        assert!(is_rtl("ar"));
        assert!(!is_rtl("en-US"));
    }
}
