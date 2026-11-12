import com.android.apksig.ApkSigner;

import java.io.File;
import java.io.FileInputStream;
import java.security.KeyStore;
import java.security.PrivateKey;
import java.security.cert.X509Certificate;
import java.util.Collections;

/**
 * Signs an APK with the key in a PKCS12 keystore, using signature scheme v2:
 * the app needs Android 8 or newer, which no longer looks at the older v1.
 */
public class Sign {
    public static void main(String[] a) throws Exception {
        String store = a[0], password = a[1], alias = a[2];
        KeyStore ks = KeyStore.getInstance("PKCS12");
        try (FileInputStream in = new FileInputStream(store)) {
            ks.load(in, password.toCharArray());
        }
        PrivateKey key = (PrivateKey) ks.getKey(alias, password.toCharArray());
        X509Certificate cert = (X509Certificate) ks.getCertificate(alias);
        ApkSigner.SignerConfig signer =
                new ApkSigner.SignerConfig.Builder("mjolnir", key, Collections.singletonList(cert)).build();
        new ApkSigner.Builder(Collections.singletonList(signer))
                .setInputApk(new File(a[3]))
                .setOutputApk(new File(a[4]))
                .setV1SigningEnabled(false)
                .setV2SigningEnabled(true)
                .setMinSdkVersion(26)
                .build()
                .sign();
    }
}
