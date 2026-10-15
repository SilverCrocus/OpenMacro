// OpenMacro's Android activity, used in place of the one dx generates
// (Dioxus.toml: [application] android_main_activity). It adds the Health
// Connect bridge that src/health.rs calls over JNI.

package dev.dioxus.main

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.graphics.Typeface
import android.net.Uri
import android.os.Bundle
import android.webkit.WebView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.appcompat.app.AppCompatActivity
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.PermissionController
import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.BodyFatRecord
import androidx.health.connect.client.records.HydrationRecord
import androidx.health.connect.client.records.NutritionRecord
import androidx.health.connect.client.records.Record
import androidx.health.connect.client.records.WeightRecord
import androidx.health.connect.client.records.metadata.Metadata
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import androidx.health.connect.client.units.Energy
import androidx.health.connect.client.units.Mass
import androidx.health.connect.client.units.Percentage
import androidx.health.connect.client.units.Volume
import java.net.HttpURLConnection
import java.net.URL
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset
import kotlin.reflect.KClass
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject

typealias BuildConfig = dev.openmacro.app.BuildConfig

class MainActivity : WryActivity() {
    private var webView: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        HealthBridge.register(this)
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
    }

    override fun onResume() {
        super.onResume()
        HealthBridge.onScreen = true
        // Tells the app it's back on screen; src/health.rs syncs then.
        webView?.evaluateJavascript("window.dispatchEvent(new Event('openmacro-resume'))", null)
    }

    override fun onPause() {
        HealthBridge.onScreen = false
        super.onPause()
    }
}

/**
 * Health Connect for the Rust side. Every call except `ask` blocks, so Rust
 * makes them from a worker thread. Results are JSON text: {"ok": value} or
 * {"error": message}.
 */
object HealthBridge {
    private val PERMISSIONS = setOf(
        HealthPermission.getReadPermission(WeightRecord::class),
        HealthPermission.getReadPermission(BodyFatRecord::class),
        HealthPermission.getWritePermission(WeightRecord::class),
        HealthPermission.getWritePermission(BodyFatRecord::class),
        HealthPermission.getWritePermission(NutritionRecord::class),
        HealthPermission.getWritePermission(HydrationRecord::class),
    )

    private var launcher: ActivityResultLauncher<Set<String>>? = null

    @Volatile
    private var asking = false

    /** Whether OpenMacro is on screen: Health Connect only lets apps read in the foreground. */
    @Volatile
    @JvmStatic
    var onScreen = false

    /** Registers the permission request; must run before the activity starts. */
    fun register(activity: ComponentActivity) {
        launcher = activity.registerForActivityResult(
            PermissionController.createRequestPermissionResultContract()
        ) { asking = false }
    }

    /** 2 when ready, 1 when Health Connect must be installed or updated, 0 when unsupported. */
    @JvmStatic
    fun status(context: Context): Int = when (HealthConnectClient.getSdkStatus(context)) {
        HealthConnectClient.SDK_AVAILABLE -> 2
        HealthConnectClient.SDK_UNAVAILABLE_PROVIDER_UPDATE_REQUIRED -> 1
        else -> 0
    }

    /** The permissions granted to OpenMacro, as names like "READ_WEIGHT". */
    @JvmStatic
    fun granted(context: Context): String = reply {
        val granted = runBlocking { client(context).permissionController.getGrantedPermissions() }
        JSONArray(granted.filter { it in PERMISSIONS }.map { it.substringAfterLast('.') })
    }

    /** Opens Health Connect's permission screen. `asking()` stays true until it closes. */
    @JvmStatic
    fun ask(activity: Activity) {
        val launcher = launcher ?: return
        asking = true
        activity.runOnUiThread {
            try {
                launcher.launch(PERMISSIONS)
            } catch (e: Exception) {
                asking = false
            }
        }
    }

    @JvmStatic
    fun asking(): Boolean = asking

    /** Opens Health Connect, or its Play Store page when it needs installing. */
    @JvmStatic
    fun open(activity: Activity) {
        val intent = if (status(activity) == 1) {
            Intent(
                Intent.ACTION_VIEW,
                Uri.parse("market://details?id=com.google.android.apps.healthdata")
            ).setPackage("com.android.vending")
        } else {
            Intent(HealthConnectClient.ACTION_HEALTH_CONNECT_SETTINGS)
        }
        activity.runOnUiThread {
            try {
                activity.startActivity(intent)
            } catch (e: Exception) {
            }
        }
    }

    /** Weights other apps saved between two dates (inclusive), as [{date, value}] in kg. */
    @JvmStatic
    fun readWeights(context: Context, from: String, to: String): String = reply {
        read(context, WeightRecord::class, from, to) { r ->
            JSONObject().put("date", localDate(r.time, r.zoneOffset)).put("value", r.weight.inKilograms)
        }
    }

    /** Body fat other apps saved between two dates (inclusive), as [{date, value}] in percent. */
    @JvmStatic
    fun readBodyFat(context: Context, from: String, to: String): String = reply {
        read(context, BodyFatRecord::class, from, to) { r ->
            JSONObject().put("date", localDate(r.time, r.zoneOffset)).put("value", r.percentage.value)
        }
    }

    /**
     * Replaces what OpenMacro saved from `from` to `to` (inclusive) with the
     * records in `json`, for the record types it may write:
     * {"food": [{date, hour, minute, name, nutrients: {field: g, energy: kcal}}],
     *  "water": [{date, hour, minute, liters}], "weight": [{date, hour, minute, kg}],
     *  "bodyFat": [{date, hour, minute, percent}]}
     */
    @JvmStatic
    fun write(context: Context, from: String, to: String, json: String): String = reply {
        val data = JSONObject(json)
        val client = client(context)
        val zone = ZoneId.systemDefault()
        val start = LocalDate.parse(from).atStartOfDay(zone).toInstant()
        val end = LocalDate.parse(to).plusDays(1).atStartOfDay(zone).toInstant()
        // Records can't be in the future; anything later than a minute ago
        // moves back to then, and is dropped if that leaves its day.
        val latest = Instant.now().minusSeconds(60)
        val granted = runBlocking { client.permissionController.getGrantedPermissions() }
        val records = mutableListOf<Record>()

        fun each(key: String, build: (JSONObject, Instant, ZoneOffset) -> Record) {
            val list = data.optJSONArray(key) ?: return
            for (i in 0 until list.length()) {
                val o = list.getJSONObject(i)
                val date = LocalDate.parse(o.getString("date"))
                val time = minOf(
                    date.atTime(o.getInt("hour"), o.optInt("minute")).atZone(zone).toInstant(),
                    latest,
                )
                if (time < date.atStartOfDay(zone).toInstant()) continue
                records.add(build(o, time, zone.rules.getOffset(time)))
            }
        }

        val types = mutableListOf<KClass<out Record>>()
        fun writable(type: KClass<out Record>): Boolean =
            (HealthPermission.getWritePermission(type) in granted).also { if (it) types.add(type) }

        if (writable(NutritionRecord::class)) {
            each("food") { o, t, offset -> nutrition(o, t, offset) }
        }
        if (writable(HydrationRecord::class)) {
            each("water") { o, t, offset ->
                HydrationRecord(
                    startTime = t, startZoneOffset = offset,
                    endTime = t.plusSeconds(60), endZoneOffset = offset,
                    volume = Volume.liters(o.getDouble("liters")),
                    metadata = Metadata.manualEntry(),
                )
            }
        }
        if (writable(WeightRecord::class)) {
            each("weight") { o, t, offset ->
                WeightRecord(
                    time = t, zoneOffset = offset,
                    weight = Mass.kilograms(o.getDouble("kg")),
                    metadata = Metadata.manualEntry(),
                )
            }
        }
        if (writable(BodyFatRecord::class)) {
            each("bodyFat") { o, t, offset ->
                BodyFatRecord(
                    time = t, zoneOffset = offset,
                    percentage = Percentage(o.getDouble("percent")),
                    metadata = Metadata.manualEntry(),
                )
            }
        }
        runBlocking {
            for (type in types) {
                client.deleteRecords(type, TimeRangeFilter.between(start, end))
            }
            for (chunk in records.chunked(500)) {
                client.insertRecords(chunk)
            }
        }
        records.size
    }

    private fun nutrition(o: JSONObject, t: Instant, offset: ZoneOffset): NutritionRecord {
        val n = o.getJSONObject("nutrients")
        fun g(key: String): Mass? = if (n.has(key)) Mass.grams(n.getDouble(key)) else null
        return NutritionRecord(
            startTime = t, startZoneOffset = offset,
            endTime = t.plusSeconds(60), endZoneOffset = offset,
            metadata = Metadata.manualEntry(),
            name = o.optString("name").ifEmpty { null },
            energy = if (n.has("energy")) Energy.kilocalories(n.getDouble("energy")) else null,
            protein = g("protein"),
            totalCarbohydrate = g("totalCarbohydrate"),
            totalFat = g("totalFat"),
            saturatedFat = g("saturatedFat"),
            transFat = g("transFat"),
            monounsaturatedFat = g("monounsaturatedFat"),
            polyunsaturatedFat = g("polyunsaturatedFat"),
            cholesterol = g("cholesterol"),
            dietaryFiber = g("dietaryFiber"),
            sugar = g("sugar"),
            sodium = g("sodium"),
            potassium = g("potassium"),
            calcium = g("calcium"),
            iron = g("iron"),
            magnesium = g("magnesium"),
            manganese = g("manganese"),
            phosphorus = g("phosphorus"),
            selenium = g("selenium"),
            copper = g("copper"),
            zinc = g("zinc"),
            vitaminA = g("vitaminA"),
            vitaminC = g("vitaminC"),
            vitaminD = g("vitaminD"),
            vitaminE = g("vitaminE"),
            vitaminK = g("vitaminK"),
            vitaminB6 = g("vitaminB6"),
            vitaminB12 = g("vitaminB12"),
            thiamin = g("thiamin"),
            riboflavin = g("riboflavin"),
            niacin = g("niacin"),
            pantothenicAcid = g("pantothenicAcid"),
            folate = g("folate"),
            caffeine = g("caffeine"),
        )
    }

    private fun client(context: Context) = HealthConnectClient.getOrCreate(context)

    private fun localDate(time: Instant, offset: ZoneOffset?): String =
        (offset?.let { time.atOffset(it).toLocalDate() }
            ?: time.atZone(ZoneId.systemDefault()).toLocalDate()).toString()

    /** Records of `type` other apps saved between two dates (inclusive), one JSON object each. */
    private fun <T : Record> read(
        context: Context,
        type: KClass<T>,
        from: String,
        to: String,
        row: (T) -> JSONObject,
    ): JSONArray {
        val client = client(context)
        val zone = ZoneId.systemDefault()
        val range = TimeRangeFilter.between(
            LocalDate.parse(from).atStartOfDay(zone).toInstant(),
            LocalDate.parse(to).plusDays(1).atStartOfDay(zone).toInstant(),
        )
        val out = JSONArray()
        var token: String? = null
        do {
            val page = runBlocking {
                client.readRecords(ReadRecordsRequest(type, range, pageToken = token))
            }
            page.records
                .filter { it.metadata.dataOrigin.packageName != context.packageName }
                .forEach { out.put(row(it)) }
            token = page.pageToken
        } while (token != null)
        return out
    }

    private fun reply(body: () -> Any): String = try {
        JSONObject().put("ok", body()).toString()
    } catch (e: Throwable) {
        val message = if (e is SecurityException || e.cause is SecurityException ||
            e.message?.contains("SecurityException") == true
        ) {
            "Health Connect didn't allow that. Check OpenMacro's permissions in Health Connect."
        } else {
            "Health Connect: ${e.message ?: e.javaClass.simpleName}"
        }
        JSONObject().put("error", message).toString()
    }
}

/**
 * Web requests for the Rust side (AI logging), made from the app rather
 * than the web view, so services that don't allow cross-site calls still
 * work. Blocks, so Rust calls it from a worker thread.
 */
object HttpBridge {
    /**
     * POSTs JSON `body` to `url` with `headers` (a JSON object of names to
     * values). Returns {"status": code, "body": text} or {"error": message}.
     */
    @JvmStatic
    fun post(url: String, headers: String, body: String): String = try {
        val connection = URL(url).openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            connection.connectTimeout = 30_000
            connection.readTimeout = 120_000
            connection.doOutput = true
            connection.setRequestProperty("content-type", "application/json")
            val names = JSONObject(headers)
            for (name in names.keys()) {
                connection.setRequestProperty(name, names.getString(name))
            }
            connection.outputStream.use { it.write(body.toByteArray()) }
            val status = connection.responseCode
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            val text = stream?.bufferedReader()?.use { it.readText() } ?: ""
            JSONObject().put("status", status).put("body", text).toString()
        } finally {
            connection.disconnect()
        }
    } catch (e: Exception) {
        JSONObject().put("error", e.message ?: e.javaClass.simpleName).toString()
    }
}

/**
 * Explains what OpenMacro does with health data. Health Connect opens this
 * from its permission screen and its app settings.
 */
class HealthPrivacyActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = (24 * resources.displayMetrics.density).toInt()
        val title = TextView(this).apply {
            textSize = 22f
            setTypeface(typeface, Typeface.BOLD)
            text = "OpenMacro and Health Connect"
        }
        val body = TextView(this).apply {
            textSize = 16f
            setPadding(0, pad / 2, 0, 0)
            text = """
                When you connect Health Connect, OpenMacro:

                • reads weight and body fat that other apps saved, such as a smart scale's app, and adds them to your weigh-ins and measurements;

                • writes your food log (calories and nutrients), water, weigh-ins and body fat, so other apps can show them.

                OpenMacro has no account or server, and doesn't send your health data anywhere: it stays in OpenMacro and Health Connect on this phone. You can turn the connection off in OpenMacro (More > Health Connect), and remove OpenMacro's access and data in Health Connect, at any time.

                OpenMacro is open source: github.com/SilverCrocus/OpenMacro
            """.trimIndent()
        }
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
            addView(title)
            addView(body)
        }
        setContentView(ScrollView(this).apply { addView(column) })
    }
}
