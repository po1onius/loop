import { useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  TextInput,
} from "react-native";
import { router, useLocalSearchParams } from "expo-router";
import { SafeAreaView } from "react-native-safe-area-context";

import { login } from "@/lib/auth-api";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";

export default function LoginScreen() {
  const params = useLocalSearchParams<{ redirect?: string | string[] }>();
  const redirect = normalizeRedirect(params.redirect);
  const [account, setAccount] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const handleLogin = async () => {
    if (!account.trim() || !password) {
      return;
    }

    setSubmitting(true);
    setError("");
    try {
      await login({
        account: account.trim(),
        password,
      });
      console.info("[login] login completed", { redirect: redirect ?? "/(tabs)" });
      router.replace((redirect ?? "/(tabs)") as never);
    } catch (e) {
      setError(e instanceof Error ? e.message : "登录失败");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={styles.flex}
      >
        <ThemedView style={styles.container}>
          <ThemedView style={styles.card} lightColor="#F5F8FC" darkColor="#1E252C">
            <ThemedText type="title" style={styles.title}>
              登录
            </ThemedText>
            <ThemedText style={styles.subtitle}>
              输入账号和密码以继续使用应用
            </ThemedText>

            <TextInput
              style={styles.input}
              placeholder="手机号 / 邮箱"
              placeholderTextColor="#8A94A6"
              autoCapitalize="none"
              value={account}
              onChangeText={setAccount}
            />
            <TextInput
              style={styles.input}
              placeholder="密码"
              placeholderTextColor="#8A94A6"
              secureTextEntry
              value={password}
              onChangeText={setPassword}
            />
            {error ? <ThemedText style={styles.errorText}>{error}</ThemedText> : null}

            <Pressable
              style={[
                styles.button,
                !account.trim() || !password || submitting ? styles.buttonDisabled : undefined,
              ]}
              onPress={handleLogin}
              disabled={!account.trim() || !password || submitting}
            >
              <ThemedText style={styles.buttonText}>
                {submitting ? "登录中..." : "登录"}
              </ThemedText>
            </Pressable>
            <Pressable
              onPress={() => router.push("/register")}
              style={styles.linkWrap}
            >
              <ThemedText style={styles.linkText}>还没有账号？去注册</ThemedText>
            </Pressable>
          </ThemedView>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function normalizeRedirect(value: string | string[] | undefined): string | null {
  const candidate = Array.isArray(value) ? value[0]?.trim() : value?.trim();
  // 仅接受应用内绝对路径，避免把登录成功后的导航参数当成外部 URL 使用。
  return candidate?.startsWith("/") && !candidate.startsWith("//")
    ? candidate
    : null;
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  flex: {
    flex: 1,
  },
  container: {
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: 20,
  },
  card: {
    borderRadius: 18,
    paddingHorizontal: 18,
    paddingVertical: 22,
    gap: 12,
  },
  title: {
    fontSize: 30,
  },
  subtitle: {
    marginBottom: 8,
    color: "#687076",
  },
  input: {
    height: 48,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#CDD3DD",
    paddingHorizontal: 14,
    fontSize: 16,
    color: "#11181C",
    backgroundColor: "#FFFFFF",
  },
  button: {
    marginTop: 8,
    height: 48,
    borderRadius: 10,
    backgroundColor: "#0A7EA4",
    alignItems: "center",
    justifyContent: "center",
  },
  buttonDisabled: {
    opacity: 0.55,
  },
  buttonText: {
    color: "#FFFFFF",
    fontWeight: "600",
  },
  errorText: {
    color: "#D64545",
  },
  linkWrap: {
    alignItems: "center",
    marginTop: 4,
    paddingVertical: 4,
  },
  linkText: {
    color: "#0A7EA4",
    fontWeight: "600",
  },
});
