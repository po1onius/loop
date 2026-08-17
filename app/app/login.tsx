import { useEffect, useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { router, useLocalSearchParams } from "expo-router";
import { SafeAreaView } from "react-native-safe-area-context";

import { login } from "@/lib/auth-api";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import {
  clearRememberedCredentials,
  isRememberPasswordAvailable,
  loadRememberedCredentials,
  saveRememberedCredentials,
} from "@/lib/remembered-credentials";

export default function LoginScreen() {
  const params = useLocalSearchParams<{
    reason?: string | string[];
    redirect?: string | string[];
  }>();
  const redirect = normalizeRedirect(params.redirect);
  const [account, setAccount] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [rememberPassword, setRememberPassword] = useState(false);
  const [rememberPasswordAvailable, setRememberPasswordAvailable] =
    useState(false);
  const [loadingRememberedCredentials, setLoadingRememberedCredentials] =
    useState(true);
  const [updatingRememberPreference, setUpdatingRememberPreference] =
    useState(false);
  const [rememberStatus, setRememberStatus] = useState("");

  useEffect(() => {
    const reason = Array.isArray(params.reason)
      ? params.reason[0]
      : params.reason;
    if (reason === "expired") {
      setError("登录凭证已过期，请重新登录");
    }
  }, [params.reason]);

  useEffect(() => {
    let active = true;

    const restoreRememberedCredentials = async () => {
      try {
        const available = await isRememberPasswordAvailable();
        if (!active) {
          return;
        }
        setRememberPasswordAvailable(available);
        if (!available) {
          setRememberStatus("当前平台不支持安全存储，请使用浏览器密码管理器");
          return;
        }

        const remembered = await loadRememberedCredentials();
        if (!active || !remembered) {
          return;
        }
        setAccount(remembered.account);
        setPassword(remembered.password);
        setRememberPassword(true);
        console.info("[login] remembered credentials restored");
      } catch (restoreError) {
        if (!active) {
          return;
        }
        const message =
          restoreError instanceof Error
            ? restoreError.message
            : "读取已保存密码失败";
        console.warn("[login] remembered credentials restore failed", {
          reason: message,
        });
        setRememberPasswordAvailable(false);
        setRememberStatus("无法读取安全存储，请手动输入账号和密码");
      } finally {
        if (active) {
          setLoadingRememberedCredentials(false);
        }
      }
    };

    void restoreRememberedCredentials();
    return () => {
      active = false;
    };
  }, []);

  const handleRememberPasswordPress = async () => {
    if (
      loadingRememberedCredentials ||
      updatingRememberPreference ||
      !rememberPasswordAvailable
    ) {
      return;
    }

    if (!rememberPassword) {
      console.info("[login] remember password enabled");
      setRememberPassword(true);
      setRememberStatus("登录成功后会将账号和密码安全保存在本机");
      return;
    }

    setUpdatingRememberPreference(true);
    setRememberStatus("");
    try {
      // 取消勾选时立即清除，不要求用户必须再完成一次登录。
      await clearRememberedCredentials();
      setRememberPassword(false);
      console.info("[login] remember password disabled");
    } catch (clearError) {
      const message =
        clearError instanceof Error ? clearError.message : "清除已保存密码失败";
      console.warn("[login] remembered credentials clear failed", {
        reason: message,
      });
      setRememberStatus(message);
    } finally {
      setUpdatingRememberPreference(false);
    }
  };

  const handleLogin = async () => {
    if (!account.trim() || !password || loadingRememberedCredentials) {
      return;
    }

    setSubmitting(true);
    setError("");
    try {
      await login({
        account: account.trim(),
        password,
      });

      if (rememberPasswordAvailable) {
        try {
          if (rememberPassword) {
            // 只有服务端确认凭据有效后才保存，避免记住输错的密码。
            await saveRememberedCredentials({
              account: account.trim(),
              password,
            });
          } else {
            await clearRememberedCredentials();
          }
        } catch (rememberError) {
          // 安全存储失败不应把已经成功的登录误报为失败，记录完整原因后继续进入应用。
          console.warn("[login] remember password synchronization failed", {
            reason:
              rememberError instanceof Error
                ? rememberError.message
                : String(rememberError),
          });
        }
      }
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
              autoComplete="username"
              textContentType="username"
              editable={!loadingRememberedCredentials && !submitting}
              value={account}
              onChangeText={setAccount}
            />
            <TextInput
              style={styles.input}
              placeholder="密码"
              placeholderTextColor="#8A94A6"
              secureTextEntry
              autoComplete="current-password"
              textContentType="password"
              editable={!loadingRememberedCredentials && !submitting}
              value={password}
              onChangeText={setPassword}
            />
            <View style={styles.rememberSection}>
              <Pressable
                accessibilityRole="checkbox"
                accessibilityState={{
                  checked: rememberPassword,
                  disabled:
                    loadingRememberedCredentials ||
                    updatingRememberPreference ||
                    !rememberPasswordAvailable,
                }}
                disabled={
                  loadingRememberedCredentials ||
                  updatingRememberPreference ||
                  !rememberPasswordAvailable
                }
                onPress={() => void handleRememberPasswordPress()}
                style={({ pressed }) => [
                  styles.rememberRow,
                  pressed ? styles.rememberRowPressed : undefined,
                  !rememberPasswordAvailable
                    ? styles.rememberRowDisabled
                    : undefined,
                ]}
              >
                <View
                  style={[
                    styles.checkbox,
                    rememberPassword ? styles.checkboxChecked : undefined,
                  ]}
                >
                  {rememberPassword ? (
                    <IconSymbol size={16} name="checkmark" color="#FFFFFF" />
                  ) : null}
                </View>
                <ThemedText style={styles.rememberLabel}>记住密码</ThemedText>
              </Pressable>
              {loadingRememberedCredentials ? (
                <ThemedText style={styles.rememberHint}>
                  正在读取已保存的登录信息...
                </ThemedText>
              ) : rememberStatus ? (
                <ThemedText style={styles.rememberHint}>{rememberStatus}</ThemedText>
              ) : null}
            </View>
            {error ? <ThemedText style={styles.errorText}>{error}</ThemedText> : null}

            <Pressable
              style={[
                styles.button,
                !account.trim() ||
                !password ||
                submitting ||
                loadingRememberedCredentials
                  ? styles.buttonDisabled
                  : undefined,
              ]}
              onPress={handleLogin}
              disabled={
                !account.trim() ||
                !password ||
                submitting ||
                loadingRememberedCredentials
              }
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
  rememberSection: {
    gap: 3,
  },
  rememberRow: {
    minHeight: 34,
    alignSelf: "flex-start",
    flexDirection: "row",
    alignItems: "center",
    gap: 9,
    paddingRight: 8,
  },
  rememberRowPressed: {
    opacity: 0.7,
  },
  rememberRowDisabled: {
    opacity: 0.48,
  },
  checkbox: {
    width: 22,
    height: 22,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 5,
    borderWidth: 1.5,
    borderColor: "#AAB3C0",
    backgroundColor: "#FFFFFF",
  },
  checkboxChecked: {
    borderColor: "#0A7EA4",
    backgroundColor: "#0A7EA4",
  },
  rememberLabel: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: "600",
  },
  rememberHint: {
    color: "#687076",
    fontSize: 12,
    lineHeight: 17,
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
