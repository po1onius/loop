import { router } from "expo-router";
import { useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { register, verifyCode } from "@/lib/auth-api";

export default function RegisterScreen() {
  const [username, setUsername] = useState("");
  const [account, setAccount] = useState("");
  const [verifyCodeText, setVerifyCodeText] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPwd, setConfirmPwd] = useState("");
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");
  const [sendingCode, setSendingCode] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const handleSendCode = async () => {
    if (!account.trim()) {
      setError("请先输入邮箱账号");
      return;
    }

    setSendingCode(true);
    setError("");
    setInfo("");
    try {
      await verifyCode({ account: account.trim() });
      setInfo("验证码已发送，请查收");
    } catch (e) {
      setError(e instanceof Error ? e.message : "发送验证码失败");
    } finally {
      setSendingCode(false);
    }
  };

  const canSubmit =
    username.trim() &&
    account.trim() &&
    verifyCodeText.trim() &&
    password &&
    confirmPwd &&
    !submitting;

  const handleRegister = async () => {
    if (!canSubmit) {
      return;
    }
    if (password !== confirmPwd) {
      setError("两次输入的密码不一致");
      return;
    }

    if (password.length > 72) {
      setError("密码长度不能超过 72 位");
      return;
    }

    setSubmitting(true);
    setError("");
    setInfo("");
    try {
      await register({
        username: username.trim(),
        account: account.trim(),
        pwd: password,
        verify_code: verifyCodeText.trim(),
      });
      setInfo("注册成功，请返回登录");
      router.replace("/login");
    } catch (e) {
      setError(e instanceof Error ? e.message : "注册失败");
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
          <ThemedView
            style={styles.card}
            lightColor="#F5F8FC"
            darkColor="#1E252C"
          >
            <ThemedText type="title" style={styles.title}>
              注册
            </ThemedText>
            <ThemedText style={styles.subtitle}>创建一个新账号</ThemedText>

            <TextInput
              style={styles.input}
              placeholder="用户名"
              placeholderTextColor="#8A94A6"
              value={username}
              onChangeText={setUsername}
            />
            <TextInput
              style={styles.input}
              placeholder="邮箱"
              placeholderTextColor="#8A94A6"
              autoCapitalize="none"
              keyboardType="email-address"
              value={account}
              onChangeText={setAccount}
            />

            <View style={styles.codeRow}>
              <TextInput
                style={[styles.input, styles.codeInput]}
                placeholder="验证码"
                placeholderTextColor="#8A94A6"
                value={verifyCodeText}
                onChangeText={setVerifyCodeText}
              />
              <Pressable
                style={[
                  styles.codeBtn,
                  sendingCode ? styles.buttonDisabled : undefined,
                ]}
                onPress={handleSendCode}
                disabled={sendingCode}
              >
                <ThemedText style={styles.codeBtnText}>
                  {sendingCode ? "发送中..." : "获取验证码"}
                </ThemedText>
              </Pressable>
            </View>

            <TextInput
              style={styles.input}
              placeholder="密码(最多72位)"
              placeholderTextColor="#8A94A6"
              secureTextEntry
              value={password}
              onChangeText={setPassword}
            />
            <TextInput
              style={styles.input}
              placeholder="确认密码"
              placeholderTextColor="#8A94A6"
              secureTextEntry
              value={confirmPwd}
              onChangeText={setConfirmPwd}
            />

            {error ? (
              <ThemedText style={styles.errorText}>{error}</ThemedText>
            ) : null}
            {info ? (
              <ThemedText style={styles.infoText}>{info}</ThemedText>
            ) : null}

            <Pressable
              style={[
                styles.button,
                !canSubmit ? styles.buttonDisabled : undefined,
              ]}
              onPress={handleRegister}
              disabled={!canSubmit}
            >
              <ThemedText style={styles.buttonText}>
                {submitting ? "提交中..." : "注册"}
              </ThemedText>
            </Pressable>

            <Pressable
              onPress={() => router.replace("/login")}
              style={styles.linkWrap}
            >
              <ThemedText style={styles.linkText}>
                已有账号？返回登录
              </ThemedText>
            </Pressable>
          </ThemedView>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
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
  codeRow: {
    flexDirection: "row",
    gap: 8,
  },
  codeInput: {
    flex: 1,
  },
  codeBtn: {
    width: 110,
    borderRadius: 10,
    backgroundColor: "#0A7EA4",
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 10,
  },
  codeBtnText: {
    color: "#FFFFFF",
    fontSize: 14,
    fontWeight: "600",
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
  infoText: {
    color: "#0F7C3B",
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
