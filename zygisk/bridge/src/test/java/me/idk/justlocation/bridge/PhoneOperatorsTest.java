package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import org.junit.Test;

public class PhoneOperatorsTest {
    @Test
    public void heartbeatUsesPipesAndKeepsEmptyFieldsInPlace() {
        String frame = PhoneOperators.encode("中国电信", "中国电信", "46011", "46011");
        assertEquals("中国电信|中国电信|46011|46011", frame);
        assertFalse(frame.contains("\n"));
        assertEquals("|中国电信||46011", PhoneOperators.encode(null, "中国电信", null, "46011"));
        assertEquals("|||", PhoneOperators.encode(null, null, null, null));
    }

    @Test
    public void malformedNamesCannotChangeFramingOrShiftOperatorCodes() {
        assertEquals("||46011|46011", PhoneOperators.encode("a|b", "a\nb", "46011", "46011"));
        assertEquals("|||", PhoneOperators.encode("a\rb", "a\u0000b", "\t", null));
        // Quotes and backslashes are data; the companion must escape them when producing JSON.
        assertEquals("A\"B|C\\D||", PhoneOperators.encode("A\"B", "C\\D", "", ""));
    }
}
