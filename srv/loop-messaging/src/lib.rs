pub mod event;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// CloudEvents 1.0 structured JSON 的版本和媒体类型。
///
/// RabbitMQ 只负责传输，消息体始终保持自描述。这样消息进入死信队列、被导出
/// 到对象存储，或未来桥接到其他消息系统时，不依赖 AMQP properties 也能解析。
pub const CLOUD_EVENTS_SPEC_VERSION: &str = "1.0";
pub const CLOUD_EVENTS_JSON_CONTENT_TYPE: &str = "application/cloudevents+json";

/// 每一种业务消息都必须声明稳定的消息类型与 RabbitMQ routing key。
///
/// 消息类型使用反向域名风格并在末尾携带业务 schema 版本；routing key 只负责
/// RabbitMQ 路由，两者职责不同，不能让消费者根据队列名猜测消息结构。
pub trait MessageContract {
    const MESSAGE_TYPE: &'static str;
    const ROUTING_KEY: &'static str;

    /// subject 用于定位本次消息影响的业务实体，例如 `events/123`。
    fn subject(&self) -> String;

    /// 各业务契约只校验自己的 data，不在通用消费框架里散落业务规则。
    fn validate_data(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// 项目统一的 CloudEvents 1.0 structured JSON envelope。
///
/// `data` 由具体业务消息定义；其余字段由消息基础设施统一生成。扩展属性必须
/// 使用小写名称，因此 correlationid、causationid、traceparent 保持 wire name。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageEnvelope<T> {
    pub specversion: String,
    pub id: Uuid,
    #[serde(rename = "type")]
    pub ty: String,
    pub source: String,
    pub subject: String,
    pub time: DateTime<Utc>,
    pub datacontenttype: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlationid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causationid: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    pub data: T,
}

impl<T: MessageContract> MessageEnvelope<T> {
    pub fn new(id: Uuid, source: impl Into<String>, data: T) -> Self {
        Self {
            specversion: CLOUD_EVENTS_SPEC_VERSION.to_string(),
            id,
            ty: T::MESSAGE_TYPE.to_string(),
            source: source.into(),
            subject: data.subject(),
            time: Utc::now(),
            datacontenttype: "application/json".to_string(),
            // 根消息默认以自身 id 作为 correlation id。下游派生消息可通过 builder
            // 继承该值并将当前消息 id 写入 causationid，形成完整因果链。
            correlationid: Some(id.to_string()),
            causationid: None,
            traceparent: None,
            data,
        }
    }

    /// 生产端和消费端复用同一套约束检查，避免一个格式看似合法但属于其他
    /// schema 的消息被错误地解释为当前业务消息。
    pub fn validate_contract(&self) -> Result<(), MessageContractError> {
        if self.specversion != CLOUD_EVENTS_SPEC_VERSION {
            return Err(MessageContractError::UnexpectedSpecVersion {
                actual: self.specversion.clone(),
            });
        }
        if self.ty != T::MESSAGE_TYPE {
            return Err(MessageContractError::UnexpectedMessageType {
                expected: T::MESSAGE_TYPE,
                actual: self.ty.clone(),
            });
        }
        if self.id.is_nil() {
            return Err(MessageContractError::InvalidAttribute("id"));
        }
        if self.source.trim().is_empty() {
            return Err(MessageContractError::InvalidAttribute("source"));
        }
        if self.source.chars().any(char::is_whitespace) {
            return Err(MessageContractError::InvalidAttribute("source"));
        }
        let expected_subject = self.data.subject();
        if self.subject != expected_subject {
            return Err(MessageContractError::UnexpectedSubject {
                expected: expected_subject,
                actual: self.subject.clone(),
            });
        }
        if self.datacontenttype != "application/json" {
            return Err(MessageContractError::InvalidAttribute("datacontenttype"));
        }
        if self
            .correlationid
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(MessageContractError::InvalidAttribute("correlationid"));
        }
        if self
            .traceparent
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(MessageContractError::InvalidAttribute("traceparent"));
        }
        self.data
            .validate_data()
            .map_err(MessageContractError::InvalidData)?;
        Ok(())
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlationid = Some(correlation_id.into());
        self
    }

    pub fn with_causation_id(mut self, causation_id: Uuid) -> Self {
        self.causationid = Some(causation_id);
        self
    }

    pub fn with_trace_parent(mut self, trace_parent: impl Into<String>) -> Self {
        self.traceparent = Some(trace_parent.into());
        self
    }
}

#[derive(Debug, Error)]
pub enum MessageContractError {
    #[error("message attribute {0} is invalid")]
    InvalidAttribute(&'static str),
    #[error("unsupported CloudEvents specversion {actual}")]
    UnexpectedSpecVersion { actual: String },
    #[error("unexpected message type {actual}; expected {expected}")]
    UnexpectedMessageType {
        expected: &'static str,
        actual: String,
    },
    #[error("unexpected message subject {actual}; expected {expected}")]
    UnexpectedSubject { expected: String, actual: String },
    #[error("message data is invalid: {0}")]
    InvalidData(&'static str),
}

/// 生成符合 CloudEvents source URI-reference 约定的服务标识。
pub fn service_source(service_name: &str) -> String {
    format!("urn:loop:service:{service_name}")
}
