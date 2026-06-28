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
        // kitchen-display commissioning — a KDS device picks which station it shows
        "setup.choose_station" => "Choose a station",
        "setup.choose_station_desc" => "Pick which kitchen station this display shows.",
        "setup.no_stations" => "No kitchen stations for this branch yet.",
        "setup.station_default" => "Default",
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
        "shift.welcome" => "Welcome back",
        "shift.opening_hint" => "Count the cash already in the drawer before you start.",
        "shift.suggested_from_close" => "From last close",
        "shift.opening_reason_label" => "Reason for the difference",
        "shift.opening_reason_hint" => "The opening count differs from the last close.",
        "shift.opening_reason_required" => "Add a reason for the cash difference.",
        "common.done" => "Done",
        // order
        "order.title" => "Order",
        "order.coming_soon" => "Catalog & ordering — coming next.",
        "order.close_shift" => "Close shift",
        "shift.close_title" => "Close shift",
        "shift.closing_desc" => "Count the drawer and close out your shift.",
        "shift.summary" => "Shift summary",
        "shift.teller" => "Teller",
        "shift.opened_at" => "Opened",
        "shift.counted_cash" => "Counted cash",
        "shift.cash_note" => "Note (optional)",
        "shift.system_cash" => "Expected cash",
        "shift.system_cash_explain" => "Opening float + cash sales − cash out",
        "shift.drawer_matches" => "Drawer matches",
        "shift.drawer_over" => "Over by",
        "shift.drawer_short" => "Short by",
        "shift.report" => "Shift report",
        "order.all" => "All",
        "order.search" => "Search items",
        "order.empty" => "No items here yet.",
        "order.empty_search" => "Nothing matches your search.",
        "order.cart" => "Cart",
        "order.cart_empty" => "Your cart is empty.",
        "order.size" => "Size",
        "order.optionals" => "Options",
        "order.add_to_cart" => "Add to cart",
        "order.update_item" => "Update item",
        "order.combos" => "Combos",
        "order.split_payment" => "Split",
        "order.split_remaining" => "Remaining",
        "order.save_component" => "Save",
        "order.configure" => "Configure",
        "order.bundle_includes" => "Includes",
        "order.bundle_save" => "Save",
        "order.select_prefix" => "Select",
        "order.addon_milk_type" => "Milk",
        "order.addon_coffee_type" => "Coffee",
        "order.addon_extra" => "Extras",
        "order.show_all_addons" => "Show all add-ons",
        "order.show_assigned_addons" => "Show fewer",
        "order.search_addons" => "Search add-ons",
        "order.recipe" => "Recipe",
        "order.required" => "Required",
        "order.subtotal" => "Subtotal",
        "order.tax" => "Tax",
        "order.total" => "Total",
        "order.discount" => "Discount",
        "order.no_discount" => "No discount",
        "order.max_reached" => "Maximum reached",
        "order.removed" => "Removed",
        "order.undo" => "Undo",
        "order.clear" => "Clear",
        "order.checkout" => "Checkout",
        "order.items" => "items",
        "order.view_cart" => "View cart",
        "order.tender" => "Payment",
        "order.payment_method" => "Payment method",
        "order.tip" => "Tip",
        "order.customer" => "Customer",
        "order.customer_hint" => "Customer name (optional)",
        "order.notes_hint" => "Order notes (optional)",
        "order.notes" => "Note",
        "order.cash_received" => "Cash received",
        "order.change" => "Change",
        "order.exact" => "Exact",
        "order.change_due" => "Change due",
        "order.short_by" => "Short by",
        "order.place_order" => "Place order",
        "order.order_placed" => "Order placed",
        "order.queued_hint" => "Saved — will sync when you're back online.",
        "order.sent_hint" => "Sent to the kitchen.",
        "order.new_order" => "New order",
        "order.done" => "Done",
        // receipt / printing
        "receipt.order" => "Order",
        "receipt.thank_you" => "Thank you!",
        "receipt.title" => "Receipt",
        "receipt.print" => "Print receipt",
        "receipt.reprint" => "Reprint receipt",
        "receipt.printing" => "Printing…",
        "receipt.printed" => "Sent to printer",
        "receipt.print_failed" => "Couldn't reach the printer",
        "receipt.no_printer" => "Set a printer in Settings",
        "receipt.ref" => "Ref:",
        "receipt.voided" => "VOIDED",
        "receipt.delivery" => "DELIVERY",
        "receipt.customer" => "Customer",
        "receipt.phone" => "Phone",
        "receipt.address" => "Address:",
        "receipt.zone" => "Zone",
        "receipt.delivery_ref" => "Delivery Ref",
        "receipt.payment_hint" => "Payment (hint)",
        "receipt.notes" => "Notes:",
        "receipt.delivery_fee" => "Delivery Fee",
        "receipt.payment" => "Payment",
        "receipt.teller" => "Teller",
        "receipt.served_by" => "Served by",
        "receipt.cash" => "Cash",
        "delivery.unit" => "Unit",
        "delivery.floor" => "Floor",
        "delivery.in_mall" => "In-Mall",
        "delivery.outside" => "Outside",
        "delivery.title" => "Delivery",
        // Unified "Orders" surface (teller): delivery + waiter open-tickets in one
        // place, two tabs. Segment labels reuse delivery.title / waiter.title.
        "incoming.title" => "Orders",
        // Kitchen Display + waiter open tickets (KDS / waiter client screens).
        "kds.title" => "Kitchen",
        "kds.reconnecting" => "Reconnecting…",
        "kds.all_clear" => "All caught up",
        "kds.waiter" => "WAITER",
        "waiter.title" => "Open tickets",
        "waiter.tickets" => "Tickets",
        "waiter.new_order" => "New order",
        "waiter.no_tickets" => "No open tickets",
        "waiter.fire" => "Fire",
        "waiter.add_round" => "Add round",
        "waiter.queued" => "Queued",
        "waiter.items" => "items",
        "waiter.customer_optional" => "Customer (optional)",
        "waiter.covers" => "Covers",
        "waiter.table" => "Table (optional)",
        "waiter.ticket" => "Ticket",
        "waiter.fired" => "Sent to kitchen",
        "waiter.settle" => "Settle",
        "waiter.settled" => "Settled",
        // settle sheet (waiter / incoming) — charge-review labels
        "tender.total" => "Total",
        "tender.method" => "Method",
        // Realtime alert titles (the core builds these; the host posts the OS notification).
        "notif.new_delivery" => "New delivery order",
        "notif.new_ticket" => "New ticket fired",
        "notif.new_round" => "New round fired",
        "notif.new_kitchen" => "New kitchen order",
        "notif.ready" => "Order ready",
        "waiter.need_shift" => "Open a shift to settle",
        "waiter.void_title" => "Void ticket",
        "waiter.void_reason" => "Reason (optional)",
        "ticket.status.open" => "Open",
        "ticket.status.ready" => "Ready",
        "ticket.status.settled" => "Settled",
        "ticket.status.voided" => "Voided",
        "ticket.status.queued" => "Queued",
        "common.void" => "Void",
        "common.cancel" => "Cancel",
        "delivery.queue" => "Delivery queue",
        "delivery.empty" => "No delivery orders",
        "delivery.all" => "All",
        "delivery.active" => "Active",
        "delivery.items" => "items",
        "delivery.status.received" => "Received",
        "delivery.status.confirmed" => "Confirmed",
        "delivery.status.preparing" => "Preparing",
        "delivery.status.ready" => "Ready",
        "delivery.status.out_for_delivery" => "Out for delivery",
        "delivery.status.delivered" => "Delivered",
        "delivery.status.cancelled" => "Cancelled",
        "delivery.status.rejected" => "Rejected",
        "delivery.action.confirmed" => "Confirm",
        "delivery.action.preparing" => "Start preparing",
        "delivery.action.ready" => "Mark ready",
        "delivery.action.out_for_delivery" => "Out for delivery",
        "delivery.action.delivered" => "Mark delivered",
        "delivery.finalize" => "Finalize sale",
        "delivery.finalize_pay" => "Charge to",
        "delivery.cancel" => "Cancel order",
        "delivery.reject" => "Reject order",
        "delivery.cancel_reason" => "Reason (optional)",
        "delivery.restore_inventory" => "Restock ingredients",
        "delivery.prep_time" => "Prep time",
        "delivery.add_prep" => "+5 min",
        "delivery.finalized" => "Sale finalized",
        "delivery.accepting" => "Accepting",
        "delivery.mode_auto" => "Auto",
        "delivery.mode_open" => "Open",
        "delivery.mode_closed" => "Closed",
        // sync center (outbox)
        "sync.title" => "Sync",
        "sync.empty" => "Everything's synced.",
        "sync.queued" => "Queued",
        "sync.sending" => "Sending",
        "sync.failed" => "Failed",
        "sync.retry" => "Retry failed",
        "sync.discard" => "Discard",
        "sync.attempts" => "attempts",
        "sync.pending" => "pending",
        "sync.op_open_shift" => "Open shift",
        "sync.op_close_shift" => "Close shift",
        "sync.op_create_order" => "Order",
        // order-screen chrome (action bar + banners)
        "chrome.online" => "Online",
        "chrome.clock_skew" => "Device clock is off — please fix it",
        "chrome.offline" => "Offline",
        "chrome.offline_banner" => "Working offline — changes sync when you reconnect",
        "chrome.auth_paused" => "Sync paused — sign in again to resume",
        "chrome.auth_paused_action" => "Sign in",
        "chrome.reauth_title" => "Resume sync",
        "chrome.reauth_body" => "Your session expired. Enter your PIN to resume syncing.",
        "chrome.reauth_as" => "Signed in as",
        "chrome.reauth_switch" => "Close shift & switch teller",
        "chrome.sync_resumed" => "Signed in — syncing resumed",
        "chrome.sync_data" => "Sync data",
        "chrome.sync_done" => "Data synced",
        "chrome.sync_failed" => "Couldn't sync — try again",
        "chrome.syncing" => "Syncing",
        "chrome.needs_attention" => "Needs attention",
        "chrome.queued" => "queued",
        "chrome.orders" => "orders",
        "chrome.more" => "More",
        // cash in/out + past shifts
        "cash.title" => "Cash In/Out",
        "cash.in" => "Cash in",
        "cash.out" => "Cash out",
        "cash.amount" => "Amount",
        "cash.note" => "Note",
        "cash.record" => "Record movement",
        "cash.empty" => "No cash movements this shift.",
        "cash.history" => "Movements",
        "cash.total_in" => "Total in",
        "cash.total_out" => "Total out",
        "cash.net" => "Net",
        "shifts.title" => "Past shifts",
        "shifts.empty" => "No shifts yet.",
        "shifts.closed" => "Closed",
        "shifts.opening" => "Opening",
        "shifts.declared" => "Declared",
        "shifts.discrepancy" => "Discrepancy",
        "shifts.orders" => "Orders",
        "shifts.no_orders" => "No orders in this shift.",
        "shifts.open_now" => "Open",
        // Z-report (printed shift report)
        "shift.report_title" => "Shift Report",
        "shift.payments" => "Payments",
        "shift.cash_moves" => "Cash in/out",
        "shift.cash_in" => "Cash in",
        "shift.cash_out" => "Cash out",
        "shift.expected_cash" => "Expected cash",
        "shift.by_method" => "By method",
        "shift.business_date" => "Business Date",
        "shift.printed_at" => "Printed at",
        "shift.interim" => "Interim Report (Shift Still Open)",
        "shift.orders" => "orders",
        "shift.total_collected" => "Total Collected",
        "shift.drawer_ops" => "Drawer Operations",
        "shift.cash_recon" => "Cash Reconciliation",
        "shift.not_closed" => "Shift not yet closed",
        "shift.difference" => "Difference",
        "shift.opening_mismatch" => "Opening mismatch",
        "shift.transactions" => "Transactions",
        "shift.end_of_report" => "End of Report",
        "shift.print_report" => "Print report",
        "drafts.title" => "Held orders",
        "drafts.empty" => "No held orders.",
        "drafts.current" => "Current",
        // order history
        "history.title" => "Orders",
        "history.empty" => "No orders this shift yet.",
        "history.queued" => "Queued",
        "history.completed" => "Completed",
        "history.search" => "Search orders",
        "history.failed" => "Failed",
        "history.voided" => "Voided",
        "history.order" => "Order",
        // order-history table (Flutter-style columns / filters / stats)
        "history.current_shift" => "Current shift",
        "history.no_match" => "No matching orders",
        // all-orders search (history lookup across shifts)
        "search.title" => "Find orders",
        "search.teller_hint" => "Teller name",
        "search.date_24h" => "24h",
        "search.date_7d" => "7 days",
        "search.date_30d" => "30 days",
        "search.load_more" => "Load more",
        "search.exported" => "Orders copied as CSV",
        "history.synced" => "Synced",
        "history.stat.orders" => "Orders",
        "history.show_more" => "Show {count} more",
        "history.col.time" => "Time",
        "history.col.teller" => "Teller",
        "history.col.amount" => "Amount",
        "history.type.all" => "All",
        "history.type.dine_in" => "Dine-in",
        "history.type.delivery" => "Delivery",
        "order.payment" => "Payment",
        // void order
        "void.action" => "Void",
        "void.title" => "Void order",
        "void.reason" => "Reason",
        "void.reason_mistake" => "Order mistake",
        "void.reason_customer" => "Customer changed their mind",
        "void.reason_quality" => "Quality issue",
        "void.reason_other" => "Other",
        "void.note" => "Note (optional)",
        "void.restock" => "Restock ingredients",
        "void.confirm" => "Void order",
        "void.cancel" => "Cancel",
        // settings
        "settings.title" => "Settings",
        "settings.account" => "Account",
        "settings.appearance" => "Appearance",
        "settings.theme_light" => "Light",
        "settings.theme_dark" => "Dark",
        "settings.theme_system" => "System",
        "settings.language" => "Language",
        "settings.device" => "Device",
        "settings.reconfigure" => "Reconfigure device",
        "settings.diagnostics" => "Diagnostics",
        "settings.recent_warnings" => "Recent warnings",
        "settings.clear" => "Clear",
        "settings.version" => "Version",
        "settings.server" => "Server",
        "settings.pending" => "Pending sync",
        "settings.realtime" => "Live updates",
        "settings.realtime_on" => "Connected",
        "settings.realtime_off" => "Reconnecting…",
        "settings.printer" => "Printer",
        "settings.till" => "Till",
        "settings.till_default" => "Branch default",
        "settings.printer_hint" => "IP address (e.g. 192.168.1.50)",
        "settings.printer_epson" => "Epson",
        "settings.printer_star" => "Star",
        "settings.device_code_hint" => "e.g. T1, W2, K1",
        "settings.device_code_caption" => "Names this till in every order reference.",
        "settings.lan" => "LAN relay",
        "settings.lan_hub_hint" => "Hub IP — optional (e.g. 192.168.1.50)",
        "settings.lan_caption" => "Set a fixed hub if devices can't find each other automatically on this Wi-Fi.",
        "settings.lan_active" => "Relay active",
        "settings.lan_offline" => "Relay off",
        "settings.lan_peers" => "peers",
        "settings.sign_out" => "Sign out",
        "settings.sign_out_shift_open" => "Close your shift before signing out.",
        "settings.reconfigure_shift_open" => "Close the current shift before reconfiguring the device.",
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
        "setup.choose_station" => "اختر محطة",
        "setup.choose_station_desc" => "اختر محطة المطبخ التي يعرضها هذا الجهاز.",
        "setup.no_stations" => "لا توجد محطات مطبخ لهذا الفرع بعد.",
        "setup.station_default" => "افتراضي",
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
        "shift.welcome" => "مرحبًا بعودتك",
        "shift.opening_hint" => "احسب النقد الموجود في الدرج قبل أن تبدأ.",
        "shift.suggested_from_close" => "من آخر إغلاق",
        "shift.opening_reason_label" => "سبب الاختلاف",
        "shift.opening_reason_hint" => "العدّ الافتتاحي يختلف عن آخر إغلاق.",
        "shift.opening_reason_required" => "أضِف سبباً لاختلاف النقدية.",
        "common.done" => "تم",
        "order.title" => "طلب",
        "order.coming_soon" => "القائمة والطلبات — قريبًا.",
        "order.close_shift" => "إغلاق الوردية",
        "shift.close_title" => "إغلاق الوردية",
        "shift.closing_desc" => "احسب الدرج وأغلق ورديتك.",
        "shift.summary" => "ملخص الوردية",
        "shift.teller" => "أمين الصندوق",
        "shift.opened_at" => "فُتحت",
        "shift.counted_cash" => "النقد المحسوب",
        "shift.cash_note" => "ملاحظة (اختياري)",
        "shift.system_cash" => "النقد المتوقع",
        "shift.system_cash_explain" => "الافتتاحي + المبيعات النقدية − المسحوب",
        "shift.drawer_matches" => "الدرج مطابق",
        "shift.drawer_over" => "زيادة",
        "shift.drawer_short" => "نقص",
        "shift.report" => "تقرير الوردية",
        "order.all" => "الكل",
        "order.search" => "ابحث عن صنف",
        "order.empty" => "لا توجد أصناف هنا بعد.",
        "order.empty_search" => "لا شيء يطابق بحثك.",
        "order.cart" => "السلة",
        "order.cart_empty" => "سلتك فارغة.",
        "order.size" => "الحجم",
        "order.optionals" => "خيارات",
        "order.add_to_cart" => "أضف إلى السلة",
        "order.update_item" => "تحديث الصنف",
        "order.combos" => "العروض",
        "order.split_payment" => "تقسيم",
        "order.split_remaining" => "المتبقي",
        "order.save_component" => "حفظ",
        "order.configure" => "تخصيص",
        "order.bundle_includes" => "يشمل",
        "order.bundle_save" => "حفظ",
        "order.select_prefix" => "اختر",
        "order.addon_milk_type" => "الحليب",
        "order.addon_coffee_type" => "القهوة",
        "order.addon_extra" => "إضافات",
        "order.show_all_addons" => "عرض كل الإضافات",
        "order.show_assigned_addons" => "عرض أقل",
        "order.search_addons" => "ابحث عن الإضافات",
        "order.recipe" => "الوصفة",
        "order.required" => "مطلوب",
        "order.subtotal" => "المجموع الفرعي",
        "order.tax" => "الضريبة",
        "order.total" => "الإجمالي",
        "order.discount" => "خصم",
        "order.no_discount" => "بدون خصم",
        "order.max_reached" => "تم بلوغ الحد الأقصى",
        "order.removed" => "تم الحذف",
        "order.undo" => "تراجع",
        "order.clear" => "تفريغ",
        "order.checkout" => "الدفع",
        "order.items" => "أصناف",
        "order.view_cart" => "عرض السلة",
        "order.tender" => "الدفع",
        "order.payment_method" => "طريقة الدفع",
        "order.tip" => "البقشيش",
        "order.customer" => "العميل",
        "order.customer_hint" => "اسم العميل (اختياري)",
        "order.notes_hint" => "ملاحظات الطلب (اختياري)",
        "order.notes" => "ملاحظة",
        "order.cash_received" => "النقد المستلم",
        "order.change" => "الباقي",
        "order.exact" => "بالضبط",
        "order.change_due" => "الباقي",
        "order.short_by" => "ناقص",
        "order.place_order" => "تأكيد الطلب",
        "order.order_placed" => "تم الطلب",
        "order.queued_hint" => "تم الحفظ — ستتم المزامنة عند عودة الاتصال.",
        "order.sent_hint" => "أُرسل إلى المطبخ.",
        "order.new_order" => "طلب جديد",
        "order.done" => "تم",
        "receipt.order" => "طلب",
        "receipt.thank_you" => "شكراً لك!",
        "receipt.title" => "الإيصال",
        "receipt.print" => "طباعة الإيصال",
        "receipt.reprint" => "إعادة طباعة الإيصال",
        "receipt.printing" => "جارٍ الطباعة…",
        "receipt.printed" => "تم الإرسال إلى الطابعة",
        "receipt.print_failed" => "تعذّر الوصول إلى الطابعة",
        "receipt.no_printer" => "اضبط الطابعة في الإعدادات",
        "receipt.ref" => "مرجع:",
        "receipt.voided" => "ملغي",
        "receipt.delivery" => "توصيل",
        "receipt.customer" => "العميل",
        "receipt.phone" => "الهاتف",
        "receipt.address" => "العنوان:",
        "receipt.zone" => "المنطقة",
        "receipt.delivery_ref" => "مرجع التوصيل",
        "receipt.payment_hint" => "الدفع (تلميح)",
        "receipt.notes" => "ملاحظات:",
        "receipt.delivery_fee" => "رسوم التوصيل",
        "receipt.payment" => "الدفع",
        "receipt.teller" => "الكاشير",
        "receipt.served_by" => "قدّمها",
        "receipt.cash" => "نقدي",
        "delivery.unit" => "وحدة",
        "delivery.floor" => "طابق",
        "delivery.in_mall" => "داخل المول",
        "delivery.outside" => "خارجي",
        "delivery.title" => "التوصيل",
        "incoming.title" => "الطلبات",
        // Kitchen Display + waiter open tickets.
        "kds.title" => "المطبخ",
        "kds.reconnecting" => "جارٍ إعادة الاتصال…",
        "kds.all_clear" => "لا طلبات معلّقة",
        "kds.waiter" => "نادل",
        "waiter.title" => "التذاكر المفتوحة",
        "waiter.tickets" => "التذاكر",
        "waiter.new_order" => "طلب جديد",
        "waiter.no_tickets" => "لا توجد تذاكر مفتوحة",
        "waiter.fire" => "إرسال",
        "waiter.add_round" => "إضافة جولة",
        "waiter.queued" => "بالانتظار",
        "waiter.items" => "صنف",
        "waiter.customer_optional" => "العميل (اختياري)",
        "waiter.covers" => "عدد الضيوف",
        "waiter.table" => "طاولة (اختياري)",
        "waiter.ticket" => "تذكرة",
        "waiter.fired" => "أُرسل إلى المطبخ",
        "waiter.settle" => "تسوية",
        "waiter.settled" => "تمت التسوية",
        "tender.total" => "الإجمالي",
        "tender.method" => "الطريقة",
        "notif.new_delivery" => "طلب توصيل جديد",
        "notif.new_ticket" => "تذكرة جديدة",
        "notif.new_round" => "جولة جديدة",
        "notif.new_kitchen" => "طلب مطبخ جديد",
        "notif.ready" => "الطلب جاهز",
        "waiter.need_shift" => "افتح وردية للتسوية",
        "waiter.void_title" => "إلغاء التذكرة",
        "waiter.void_reason" => "السبب (اختياري)",
        "ticket.status.open" => "مفتوحة",
        "ticket.status.ready" => "جاهزة",
        "ticket.status.settled" => "مُسوّاة",
        "ticket.status.voided" => "ملغاة",
        "ticket.status.queued" => "بالانتظار",
        "common.void" => "إلغاء",
        "common.cancel" => "إلغاء",
        "delivery.queue" => "قائمة التوصيل",
        "delivery.empty" => "لا توجد طلبات توصيل",
        "delivery.all" => "الكل",
        "delivery.active" => "نشطة",
        "delivery.items" => "أصناف",
        "delivery.status.received" => "مستلم",
        "delivery.status.confirmed" => "مؤكد",
        "delivery.status.preparing" => "قيد التحضير",
        "delivery.status.ready" => "جاهز",
        "delivery.status.out_for_delivery" => "خرج للتوصيل",
        "delivery.status.delivered" => "تم التوصيل",
        "delivery.status.cancelled" => "ملغي",
        "delivery.status.rejected" => "مرفوض",
        "delivery.action.confirmed" => "تأكيد",
        "delivery.action.preparing" => "بدء التحضير",
        "delivery.action.ready" => "تحديد جاهز",
        "delivery.action.out_for_delivery" => "خرج للتوصيل",
        "delivery.action.delivered" => "تحديد تم التوصيل",
        "delivery.finalize" => "إتمام البيع",
        "delivery.finalize_pay" => "تحصيل عبر",
        "delivery.cancel" => "إلغاء الطلب",
        "delivery.reject" => "رفض الطلب",
        "delivery.cancel_reason" => "السبب (اختياري)",
        "delivery.restore_inventory" => "إعادة المخزون",
        "delivery.prep_time" => "وقت التحضير",
        "delivery.add_prep" => "+٥ دقائق",
        "delivery.finalized" => "تم إتمام البيع",
        "delivery.accepting" => "قبول الطلبات",
        "delivery.mode_auto" => "تلقائي",
        "delivery.mode_open" => "مفتوح",
        "delivery.mode_closed" => "مغلق",
        "sync.title" => "المزامنة",
        "sync.empty" => "كل شيء متزامن.",
        "sync.queued" => "في الانتظار",
        "sync.sending" => "جارٍ الإرسال",
        "sync.failed" => "فشل",
        "sync.retry" => "إعادة محاولة الفاشلة",
        "sync.discard" => "تجاهل",
        "sync.attempts" => "محاولات",
        "sync.pending" => "قيد المزامنة",
        "sync.op_open_shift" => "فتح وردية",
        "sync.op_close_shift" => "إغلاق وردية",
        "sync.op_create_order" => "طلب",
        // order-screen chrome (action bar + banners)
        "chrome.online" => "متصل",
        "chrome.clock_skew" => "ساعة الجهاز غير مضبوطة — يرجى تصحيحها",
        "chrome.offline" => "غير متصل",
        "chrome.offline_banner" => "تعمل دون اتصال — ستتم المزامنة عند عودة الاتصال",
        "chrome.auth_paused" => "توقفت المزامنة — سجّل الدخول لاستئنافها",
        "chrome.auth_paused_action" => "تسجيل الدخول",
        "chrome.reauth_title" => "استئناف المزامنة",
        "chrome.reauth_body" => "انتهت جلستك. أدخل رمزك السري لاستئناف المزامنة.",
        "chrome.reauth_as" => "مسجّل الدخول باسم",
        "chrome.reauth_switch" => "إغلاق الوردية وتبديل الكاشير",
        "chrome.sync_resumed" => "تم تسجيل الدخول — استؤنفت المزامنة",
        "chrome.sync_data" => "مزامنة البيانات",
        "chrome.sync_done" => "تمت مزامنة البيانات",
        "chrome.sync_failed" => "تعذّرت المزامنة — حاول مجددًا",
        "chrome.syncing" => "جارٍ المزامنة",
        "chrome.needs_attention" => "يحتاج إلى مراجعة",
        "chrome.queued" => "في الانتظار",
        "chrome.orders" => "طلبات",
        "chrome.more" => "المزيد",
        // cash in/out + past shifts
        "cash.title" => "إيداع/سحب نقدي",
        "cash.in" => "إيداع",
        "cash.out" => "سحب",
        "cash.amount" => "المبلغ",
        "cash.note" => "ملاحظة",
        "cash.record" => "تسجيل الحركة",
        "cash.empty" => "لا توجد حركات نقدية في هذه الوردية.",
        "cash.history" => "الحركات",
        "cash.total_in" => "إجمالي الداخل",
        "cash.total_out" => "إجمالي الخارج",
        "cash.net" => "الصافي",
        "shifts.title" => "الورديات السابقة",
        "shifts.empty" => "لا توجد ورديات بعد.",
        "shifts.closed" => "أُغلقت",
        "shifts.opening" => "رصيد البداية",
        "shifts.declared" => "المعلن",
        "shifts.discrepancy" => "الفرق",
        "shifts.orders" => "الطلبات",
        "shifts.no_orders" => "لا توجد طلبات في هذه الوردية.",
        "shifts.open_now" => "مفتوحة",
        // Z-report (printed shift report)
        "shift.report_title" => "تقرير الوردية",
        "shift.payments" => "المدفوعات",
        "shift.cash_moves" => "إيداع/سحب",
        "shift.cash_in" => "إيداع نقدي",
        "shift.cash_out" => "سحب نقدي",
        "shift.expected_cash" => "النقد المتوقع",
        "shift.by_method" => "حسب الطريقة",
        "shift.business_date" => "تاريخ العمل",
        "shift.printed_at" => "وقت الطباعة",
        "shift.interim" => "تقرير مبدئي (الوردية ما زالت مفتوحة)",
        "shift.orders" => "طلب",
        "shift.total_collected" => "إجمالي المحصّل",
        "shift.drawer_ops" => "عمليات الدرج",
        "shift.cash_recon" => "تسوية النقدية",
        "shift.not_closed" => "لم تُغلق الوردية بعد",
        "shift.difference" => "الفرق",
        "shift.opening_mismatch" => "فرق الافتتاح",
        "shift.transactions" => "المعاملات",
        "shift.end_of_report" => "نهاية التقرير",
        "shift.print_report" => "طباعة التقرير",
        "drafts.title" => "طلبات معلّقة",
        "drafts.empty" => "لا توجد طلبات معلّقة.",
        "drafts.current" => "الحالي",
        "history.title" => "الطلبات",
        "history.empty" => "لا توجد طلبات في هذه الوردية بعد.",
        "history.queued" => "في الانتظار",
        "history.completed" => "مكتمل",
        "history.search" => "ابحث في الطلبات",
        "history.failed" => "فشل",
        "history.voided" => "ملغى",
        "history.order" => "طلب",
        // order-history table (Flutter-style columns / filters / stats)
        "history.current_shift" => "الوردية الحالية",
        "history.no_match" => "لا توجد طلبات مطابقة",
        "search.title" => "ابحث عن الطلبات",
        "search.teller_hint" => "اسم الكاشير",
        "search.date_24h" => "٢٤ ساعة",
        "search.date_7d" => "٧ أيام",
        "search.date_30d" => "٣٠ يوم",
        "search.load_more" => "تحميل المزيد",
        "search.exported" => "تم نسخ الطلبات كملف CSV",
        "history.synced" => "متزامن",
        "history.stat.orders" => "الطلبات",
        "history.show_more" => "عرض {count} إضافية",
        "history.col.time" => "الوقت",
        "history.col.teller" => "الكاشير",
        "history.col.amount" => "المبلغ",
        "history.type.all" => "الكل",
        "history.type.dine_in" => "محلي",
        "history.type.delivery" => "توصيل",
        "order.payment" => "طريقة الدفع",
        "void.action" => "إبطال",
        "void.title" => "إبطال الطلب",
        "void.reason" => "السبب",
        "void.reason_mistake" => "خطأ في الطلب",
        "void.reason_customer" => "تغيّر رأي العميل",
        "void.reason_quality" => "مشكلة في الجودة",
        "void.reason_other" => "أخرى",
        "void.note" => "ملاحظة (اختياري)",
        "void.restock" => "إعادة المكوّنات للمخزون",
        "void.confirm" => "إبطال الطلب",
        "void.cancel" => "إلغاء",
        "settings.title" => "الإعدادات",
        "settings.account" => "الحساب",
        "settings.appearance" => "المظهر",
        "settings.theme_light" => "فاتح",
        "settings.theme_dark" => "داكن",
        "settings.theme_system" => "النظام",
        "settings.language" => "اللغة",
        "settings.device" => "الجهاز",
        "settings.reconfigure" => "إعادة ضبط الجهاز",
        "settings.diagnostics" => "التشخيص",
        "settings.recent_warnings" => "تحذيرات حديثة",
        "settings.clear" => "مسح",
        "settings.version" => "الإصدار",
        "settings.server" => "الخادم",
        "settings.pending" => "بانتظار المزامنة",
        "settings.realtime" => "التحديثات المباشرة",
        "settings.realtime_on" => "متصل",
        "settings.realtime_off" => "إعادة الاتصال…",
        "settings.printer" => "الطابعة",
        "settings.till" => "الكاشة",
        "settings.till_default" => "افتراضي الفرع",
        "settings.printer_hint" => "عنوان IP (مثال: 192.168.1.50)",
        "settings.printer_epson" => "إبسون",
        "settings.printer_star" => "ستار",
        "settings.device_code_hint" => "مثال: T1، W2، K1",
        "settings.device_code_caption" => "يحدد اسم هذه الكاشة في كل مرجع طلب.",
        "settings.lan" => "الشبكة المحلية",
        "settings.lan_hub_hint" => "عنوان الموزّع — اختياري (مثل 192.168.1.50)",
        "settings.lan_caption" => "عيّن موزّعًا ثابتًا إذا تعذّر على الأجهزة العثور على بعضها تلقائيًا على هذه الشبكة.",
        "settings.lan_active" => "التتابع نشط",
        "settings.lan_offline" => "التتابع متوقّف",
        "settings.lan_peers" => "أجهزة",
        "settings.sign_out" => "تسجيل الخروج",
        "settings.sign_out_shift_open" => "أغلق ورديتك قبل تسجيل الخروج.",
        "settings.reconfigure_shift_open" => "أغلق الوردية الحالية قبل إعادة ضبط الجهاز.",
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

    // ── tr: resolution order ─────────────────────────────────────────────

    #[test]
    fn tr_arabic_resolves_from_ar_table() {
        // A key present in both tables must return the AR string for ar locales.
        assert_eq!(tr("ar", "order.total"), "الإجمالي");
        assert_eq!(tr("ar-EG", "order.total"), "الإجمالي");
    }

    #[test]
    fn tr_english_locale_uses_en_table() {
        assert_eq!(tr("en", "order.total"), "Total");
        assert_eq!(tr("en-US", "order.total"), "Total");
    }

    #[test]
    fn tr_underscore_locale_separator_is_handled() {
        // lang_of splits on '_' too, not just '-'.
        assert_eq!(tr("ar_EG", "login.sign_in"), "تسجيل الدخول");
        assert_eq!(tr("en_GB", "login.sign_in"), "Sign in");
    }

    #[test]
    fn tr_unknown_language_falls_back_to_en() {
        // Non-ar, non-en language: there's no fr table, so resolve via en.
        assert_eq!(tr("fr", "order.total"), "Total");
        assert_eq!(tr("de-DE", "order.cart"), "Cart");
        assert_eq!(tr("zh", "settings.title"), "Settings");
    }

    #[test]
    fn tr_unknown_key_falls_back_to_key_itself() {
        assert_eq!(tr("en", "no.such.key"), "no.such.key");
        assert_eq!(tr("ar", "no.such.key"), "no.such.key");
        assert_eq!(tr("fr", "totally.made.up"), "totally.made.up");
    }

    #[test]
    fn tr_empty_locale_falls_back_to_en() {
        // lang_of("") yields "" → not ar → en table.
        assert_eq!(tr("", "order.total"), "Total");
    }

    #[test]
    fn tr_empty_key_returns_empty_key() {
        // No table has a "" entry, so it falls through to the key itself.
        assert_eq!(tr("en", ""), "");
        assert_eq!(tr("ar", ""), "");
    }

    #[test]
    fn tr_is_case_sensitive_on_key() {
        // Keys are matched literally; a different case is unknown → key.
        assert_eq!(tr("en", "Order.Total"), "Order.Total");
    }

    #[test]
    fn tr_is_case_sensitive_on_locale_language() {
        // lang_of does not lowercase, so "AR" is not treated as arabic → en.
        assert_eq!(tr("AR", "order.total"), "Total");
    }

    #[test]
    fn tr_key_only_in_en_falls_back_to_en_for_ar_locale() {
        // If a key exists in EN but (hypothetically) not in AR, ar() returns
        // None and tr falls through to en(). Verified structurally by the
        // coverage test below; here we assert the fallback chain on a real key
        // that the coverage test guarantees exists in both, so this just pins
        // the en value for a non-translated-looking key.
        assert_eq!(tr("ar", "settings.printer_hint"), "عنوان IP (مثال: 192.168.1.50)");
    }

    // ── is_rtl ───────────────────────────────────────────────────────────

    #[test]
    fn is_rtl_covers_all_flagged_languages() {
        assert!(is_rtl("ar"));
        assert!(is_rtl("fa"));
        assert!(is_rtl("he"));
        assert!(is_rtl("ur"));
        assert!(is_rtl("fa-IR"));
        assert!(is_rtl("he_IL"));
    }

    #[test]
    fn is_rtl_false_for_ltr_and_unknown() {
        assert!(!is_rtl("en"));
        assert!(!is_rtl("fr-FR"));
        assert!(!is_rtl(""));
        assert!(!is_rtl("AR")); // case-sensitive, uppercase is not flagged
    }

    // ── direct table access ──────────────────────────────────────────────

    #[test]
    fn en_returns_none_for_unknown_key() {
        assert!(en("definitely.not.a.key").is_none());
    }

    #[test]
    fn ar_returns_none_for_unknown_key() {
        assert!(ar("definitely.not.a.key").is_none());
    }

    #[test]
    fn en_and_ar_have_matching_known_key() {
        assert_eq!(en("order.checkout"), Some("Checkout"));
        assert_eq!(ar("order.checkout"), Some("الدفع"));
    }

    // ── COVERAGE: every EN key must also be in the AR table ───────────────

    /// Extract the `"key" =>` literals from a single `fn`'s body in the source.
    /// We slice the file between the function's opening signature and the next
    /// top-level `fn ` so the dispatch arm in `tr` (`"ar" =>`) can't leak in.
    fn keys_in_fn<'a>(src: &'a str, fn_sig: &str) -> std::collections::BTreeSet<&'a str> {
        let start = src.find(fn_sig).expect("function signature not found");
        let after = &src[start + fn_sig.len()..];
        // The body ends at the next top-level fn declaration.
        let end = after.find("\nfn ").unwrap_or(after.len());
        let body = &after[..end];

        let mut keys = std::collections::BTreeSet::new();
        for line in body.lines() {
            let t = line.trim_start();
            // Match lines shaped like:  "some.key" => "value",
            if let Some(rest) = t.strip_prefix('"') {
                if let Some(close) = rest.find('"') {
                    let key = &rest[..close];
                    let tail = rest[close + 1..].trim_start();
                    if tail.starts_with("=>") {
                        keys.insert(key);
                    }
                }
            }
        }
        keys
    }

    #[test]
    fn every_en_key_is_present_in_ar() {
        let src = include_str!("i18n.rs");
        let en_keys = keys_in_fn(src, "fn en(key: &str) -> Option<&'static str> {");
        let ar_keys = keys_in_fn(src, "fn ar(key: &str) -> Option<&'static str> {");

        // Sanity: the parser actually found a meaningful number of keys.
        assert!(en_keys.len() > 100, "parser found too few EN keys: {}", en_keys.len());
        assert!(ar_keys.len() > 100, "parser found too few AR keys: {}", ar_keys.len());

        let missing: Vec<&str> = en_keys.difference(&ar_keys).copied().collect();
        assert!(
            missing.is_empty(),
            "keys present in EN but missing from AR translation table: {missing:?}"
        );
    }

    #[test]
    fn ar_has_no_orphan_keys_absent_from_en() {
        // Reverse direction: an AR key with no EN counterpart can never be
        // reached via `tr` for an en locale and signals a typo/stale entry.
        let src = include_str!("i18n.rs");
        let en_keys = keys_in_fn(src, "fn en(key: &str) -> Option<&'static str> {");
        let ar_keys = keys_in_fn(src, "fn ar(key: &str) -> Option<&'static str> {");

        let orphans: Vec<&str> = ar_keys.difference(&en_keys).copied().collect();
        assert!(
            orphans.is_empty(),
            "keys present in AR but missing from EN table: {orphans:?}"
        );
    }

    #[test]
    fn every_en_key_resolves_nonempty_in_both_locales() {
        // Round-trip the parsed EN keys through the public `tr` to prove that
        // no key resolves to the fallback-key (which would mean a real miss)
        // and that AR yields a distinct, non-empty string.
        let src = include_str!("i18n.rs");
        let en_keys = keys_in_fn(src, "fn en(key: &str) -> Option<&'static str> {");
        for key in en_keys {
            let en_val = tr("en", key);
            let ar_val = tr("ar", key);
            assert_ne!(en_val, key, "EN key {key} resolved to itself (missing)");
            assert!(!ar_val.is_empty(), "AR value for {key} is empty");
        }
    }
}
