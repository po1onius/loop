const { expo } = require('./app.json');

module.exports = {
  ...expo,
  ios: {
    ...expo.ios,
    bundleIdentifier: process.env.LOOP_IOS_BUNDLE_ID || 'com.anonymous.loop',
    googleServicesFile: process.env.LOOP_GOOGLE_SERVICES_IOS || './GoogleService-Info.plist',
    entitlements: { 'aps-environment': process.env.LOOP_APNS_ENVIRONMENT || 'development' },
    infoPlist: { UIBackgroundModes: ['remote-notification'] },
  },
  android: {
    ...expo.android,
    package: process.env.LOOP_ANDROID_PACKAGE || expo.android.package,
    googleServicesFile: process.env.LOOP_GOOGLE_SERVICES_ANDROID || './google-services.json',
  },
  plugins: [
    ...expo.plugins.filter((plugin) => !['expo-build-properties', 'expo-notifications'].includes(typeof plugin === 'string' ? plugin : plugin[0])),
    '@react-native-firebase/app',
    '@react-native-firebase/messaging',
    ['expo-build-properties', { ios: { useFrameworks: 'static' } }],
    ['expo-notifications', { defaultChannel: 'messages' }],
  ],
};
