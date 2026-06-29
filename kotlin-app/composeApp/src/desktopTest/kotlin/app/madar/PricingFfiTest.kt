package app.madar

import app.madar.core.AddonSel
import app.madar.core.CartLine
import app.madar.core.DiscountKind
import app.madar.core.OptionalSel
import app.madar.core.PriceCartInput
import app.madar.core.PricedBreakdown
import app.madar.core.priceCart
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Client-side FFI tests for the Rust money engine, run on the desktop JVM via JNA
 * (`./gradlew :composeApp:desktopTest`). Loads `libmadar_core` from
 * `rust-core/target/debug` (jna.library.path, set in build.gradle.kts) and drives
 * the SAME pricing function the property tests cover on the Rust side — but across
 * the real UniFFI boundary the Compose app uses. Mirrors the Swift CoreFFITests.
 */
class PricingFfiTest {

    private fun line(unit: Long, qty: Long) = CartLine(
        quantity = qty, unitPrice = unit, isBundle = false,
        addons = emptyList(), optionals = emptyList(), bundleComponents = emptyList(),
    )

    private fun priced(
        lines: List<CartLine>,
        discount: DiscountKind = DiscountKind.NONE,
        value: Long = 0,
        tax: Double = 0.0,
        tender: Long? = null,
        tip: Long = 0,
    ): PricedBreakdown = priceCart(
        PriceCartInput(
            lines = lines, discountKind = discount, discountValue = value,
            taxRate = tax, amountTendered = tender, cashTip = tip,
        )
    )

    @Test
    fun pricingMatchesBackendMath() {
        val b = priced(listOf(line(1000, 2)), tax = 0.14, tender = 2500)
        assertEquals(2000, b.subtotalMinor)
        assertEquals(280, b.taxMinor)   // round(2000 * 0.14)
        assertEquals(2280, b.totalMinor)
        assertEquals(220, b.changeGivenMinor)
    }

    @Test
    fun overDiscountNeverGoesNegative() {
        val d = priced(listOf(line(1000, 1)), discount = DiscountKind.PERCENTAGE, value = 150, tax = 0.1)
        assertTrue(d.discountMinor <= d.subtotalMinor, "discount must not exceed subtotal")
        assertTrue(d.totalMinor >= 0, "total must never go negative")
        assertEquals(0, d.taxableMinor)
    }

    @Test
    fun changeFlooredAtZero() {
        assertEquals(0, priced(listOf(line(5000, 1)), tender = 100).changeGivenMinor)
    }

    @Test
    fun emptyCartIsZero() {
        assertEquals(0, priced(emptyList()).totalMinor)
    }

    @Test
    fun addonsAndOptionalsFoldIntoLine() {
        val withExtras = priceCart(
            PriceCartInput(
                lines = listOf(
                    CartLine(
                        quantity = 1, unitPrice = 1000, isBundle = false,
                        addons = listOf(AddonSel(priceModifier = 250, quantity = 2)),
                        optionals = listOf(OptionalSel(price = 100)),
                        bundleComponents = emptyList(),
                    )
                ),
                discountKind = DiscountKind.NONE, discountValue = 0, taxRate = 0.0,
                amountTendered = null, cashTip = 0,
            )
        )
        assertEquals(1600, withExtras.subtotalMinor) // 1000 + 2*250 + 100
    }
}
