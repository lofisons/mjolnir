package com.lofisons.mjolnir;

import android.Manifest;
import android.app.Activity;
import android.content.ContentValues;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.MediaStore;
import android.util.Base64;
import android.view.WindowManager;
import android.webkit.JavascriptInterface;
import android.webkit.MimeTypeMap;
import android.webkit.PermissionRequest;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Toast;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.HashMap;
import java.util.Map;

/**
 * Mjolnir on Android: one window showing the same app that runs in a browser,
 * served from inside the package. The engine is the same WebAssembly module.
 * This class only lends it what a page cannot have by itself: files in and
 * out, the microphone, the back button.
 */
public class MainActivity extends Activity {
    /** The app answers at this address, from its own assets: nothing goes to the network. */
    private static final String HOST = "mjolnir.app";
    private static final String HOME = "https://" + HOST + "/index.html";
    private static final int PICK_FILE = 1;
    private static final int ASK_MICROPHONE = 2;

    private WebView web;
    private boolean loaded;
    /** A set handed over by another app, waiting for the page. */
    private String waitingSet;
    private ValueCallback<Uri[]> chooser;
    private PermissionRequest microphone;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        // an instrument's screen stays awake
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        setVolumeControlStream(android.media.AudioManager.STREAM_MUSIC);

        web = new WebView(this);
        web.setBackgroundColor(0xff0b0b0a);
        web.setOverScrollMode(WebView.OVER_SCROLL_NEVER);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setDatabaseEnabled(true);
        s.setMediaPlaybackRequiresUserGesture(false);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        s.setSupportZoom(false);
        s.setTextZoom(100);
        web.addJavascriptInterface(new Host(), "MjolnirHost");
        web.setWebViewClient(new Pages());
        web.setWebChromeClient(new Chrome());
        setContentView(web);

        if (state != null) {
            web.restoreState(state);
        } else {
            web.loadUrl(HOME);
        }
        take(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        take(intent);
    }

    /** A set opened from a file manager or a message becomes a set in the library. */
    private void take(Intent intent) {
        if (intent == null || !Intent.ACTION_VIEW.equals(intent.getAction()) || intent.getData() == null) {
            return;
        }
        try (InputStream in = getContentResolver().openInputStream(intent.getData())) {
            if (in == null) {
                return;
            }
            String text = new String(readAll(in), "UTF-8");
            if (loaded) {
                hand(text);
            } else {
                waitingSet = text;
            }
        } catch (IOException | SecurityException e) {
            Toast.makeText(this, "that file could not be read", Toast.LENGTH_SHORT).show();
        }
    }

    private void hand(String set) {
        web.evaluateJavascript(
                "window.dispatchEvent(new CustomEvent('mjolnir-open', { detail: " + quote(set) + " }))", null);
    }

    /** A string as a JavaScript literal. */
    private static String quote(String s) {
        StringBuilder b = new StringBuilder(s.length() + 16).append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': b.append("\\\""); break;
                case '\\': b.append("\\\\"); break;
                case '\n': b.append("\\n"); break;
                case '\r': b.append("\\r"); break;
                case ' ': b.append("\\u2028"); break;
                case ' ': b.append("\\u2029"); break;
                default:
                    if (c < 0x20) {
                        b.append(String.format("\\u%04x", (int) c));
                    } else {
                        b.append(c);
                    }
            }
        }
        return b.append('"').toString();
    }

    private static byte[] readAll(InputStream in) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[65536];
        int n;
        while ((n = in.read(buf)) > 0) {
            out.write(buf, 0, n);
        }
        return out.toByteArray();
    }

    /** Serves the app from the package's assets. */
    private class Pages extends WebViewClient {
        @Override
        public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
            Uri url = request.getUrl();
            if (!HOST.equals(url.getHost())) {
                // the app needs nothing from the network
                return new WebResourceResponse("text/plain", "utf-8", 403, "Forbidden", null, null);
            }
            String path = url.getPath();
            if (path == null || path.equals("/")) {
                path = "/index.html";
            }
            try {
                InputStream in = getAssets().open("app" + path);
                Map<String, String> headers = new HashMap<String, String>();
                headers.put("Cache-Control", "no-cache");
                return new WebResourceResponse(mime(path), null, 200, "OK", headers, in);
            } catch (IOException e) {
                return new WebResourceResponse("text/plain", "utf-8", 404, "Not Found", null, null);
            }
        }

        @Override
        public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
            Uri url = request.getUrl();
            if (HOST.equals(url.getHost())) {
                return false;
            }
            // a link to somewhere else opens in the browser
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, url));
            } catch (RuntimeException ignored) {
                // no browser: stay where we are
            }
            return true;
        }

        @Override
        public void onPageFinished(WebView view, String url) {
            loaded = true;
            if (waitingSet != null) {
                final String set = waitingSet;
                waitingSet = null;
                // the engine needs a moment to wake
                view.postDelayed(new Runnable() {
                    @Override
                    public void run() {
                        hand(set);
                    }
                }, 900);
            }
        }
    }

    private static String mime(String path) {
        if (path.endsWith(".wasm")) {
            return "application/wasm";
        }
        if (path.endsWith(".js")) {
            return "text/javascript";
        }
        if (path.endsWith(".mjolnir") || path.endsWith(".webmanifest")) {
            return "application/json";
        }
        if (path.endsWith(".woff2")) {
            return "font/woff2";
        }
        String ext = MimeTypeMap.getFileExtensionFromUrl(path);
        String known = ext == null ? null : MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
        return known != null ? known : "application/octet-stream";
    }

    /** File pickers and the microphone. */
    private class Chrome extends WebChromeClient {
        @Override
        public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
            if (chooser != null) {
                chooser.onReceiveValue(null);
            }
            chooser = callback;
            Intent pick = new Intent(Intent.ACTION_GET_CONTENT);
            pick.addCategory(Intent.CATEGORY_OPENABLE);
            pick.setType("*/*");
            try {
                startActivityForResult(Intent.createChooser(pick, "open"), PICK_FILE);
            } catch (RuntimeException e) {
                chooser = null;
                return false;
            }
            return true;
        }

        @Override
        public void onPermissionRequest(final PermissionRequest request) {
            boolean wantsMicrophone = false;
            for (String r : request.getResources()) {
                if (PermissionRequest.RESOURCE_AUDIO_CAPTURE.equals(r)) {
                    wantsMicrophone = true;
                }
            }
            if (!wantsMicrophone) {
                request.deny();
                return;
            }
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                request.grant(new String[] {PermissionRequest.RESOURCE_AUDIO_CAPTURE});
            } else {
                microphone = request;
                requestPermissions(new String[] {Manifest.permission.RECORD_AUDIO}, ASK_MICROPHONE);
            }
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_FILE || chooser == null) {
            return;
        }
        Uri picked = result == RESULT_OK && data != null ? data.getData() : null;
        chooser.onReceiveValue(picked == null ? null : new Uri[] {picked});
        chooser = null;
    }

    @Override
    public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request != ASK_MICROPHONE || microphone == null) {
            return;
        }
        if (results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) {
            microphone.grant(new String[] {PermissionRequest.RESOURCE_AUDIO_CAPTURE});
        } else {
            microphone.deny();
        }
        microphone = null;
    }

    /**
     * What the page asks the device for: saving a file. A page inside an app
     * cannot download; it hands the file over in pieces and it lands in
     * Downloads/Mjolnir.
     */
    private class Host {
        private ByteArrayOutputStream pending;
        private String name;
        private String type;

        @JavascriptInterface
        public synchronized void begin(String fileName, String mimeType) {
            name = fileName.replaceAll("[\\\\/:*?\"<>|]", "_");
            type = mimeType;
            pending = new ByteArrayOutputStream();
        }

        @JavascriptInterface
        public synchronized void append(String base64) {
            if (pending == null) {
                return;
            }
            byte[] bytes = Base64.decode(base64, Base64.DEFAULT);
            pending.write(bytes, 0, bytes.length);
        }

        /** Returns where the file went, or an empty string when it could not be saved. */
        @JavascriptInterface
        public synchronized String finish() {
            if (pending == null) {
                return "";
            }
            byte[] bytes = pending.toByteArray();
            pending = null;
            try {
                if (Build.VERSION.SDK_INT >= 29) {
                    ContentValues row = new ContentValues();
                    row.put(MediaStore.MediaColumns.DISPLAY_NAME, name);
                    row.put(MediaStore.MediaColumns.MIME_TYPE, type);
                    row.put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Mjolnir");
                    Uri uri = getContentResolver().insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, row);
                    if (uri == null) {
                        return "";
                    }
                    try (OutputStream out = getContentResolver().openOutputStream(uri)) {
                        if (out == null) {
                            return "";
                        }
                        out.write(bytes);
                    }
                    return "Downloads/Mjolnir/" + name;
                }
                File dir = new File(getExternalFilesDir(null), "Mjolnir");
                if (!dir.exists() && !dir.mkdirs()) {
                    return "";
                }
                File file = new File(dir, name);
                try (FileOutputStream out = new FileOutputStream(file)) {
                    out.write(bytes);
                }
                return file.getAbsolutePath();
            } catch (IOException | RuntimeException e) {
                return "";
            }
        }
    }

    @Override
    public void onBackPressed() {
        // the page goes back a level if it can; at the top, the app steps aside
        web.evaluateJavascript("window.mjolnirBack ? window.mjolnirBack() : false", new ValueCallback<String>() {
            @Override
            public void onReceiveValue(String handled) {
                if (!"true".equals(handled)) {
                    moveTaskToBack(true);
                }
            }
        });
    }

    @Override
    protected void onSaveInstanceState(Bundle state) {
        super.onSaveInstanceState(state);
        web.saveState(state);
    }

    @Override
    protected void onPause() {
        // leaving the app stops the music and lets the set be kept
        web.evaluateJavascript("window.mjolnirPause && window.mjolnirPause()", null);
        web.onPause();
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        web.onResume();
    }

    @Override
    protected void onDestroy() {
        web.destroy();
        super.onDestroy();
    }
}
