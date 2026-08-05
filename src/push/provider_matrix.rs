//! Provider matrix for intelligent task distribution across Git providers.
//! 
//! This module implements a robust task-to-provider mapping system that ensures
//! all development tasks are distributed by best fit across SSH-accessible providers.

use crate::config::loader::RemoteConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Task types that can be distributed across providers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    // Development tasks
    FeatureDevelopment,
    BugFix,
    Refactoring,
    Documentation,
    
    // Security tasks
    SecurityAudit,
    VulnerabilityFix,
    ComplianceCheck,
    SecretScanning,
    
    // CI/CD tasks
    ContinuousIntegration,
    ContinuousDeployment,
    MutationTesting,
    PerformanceTesting,
    SecurityScanning,
    
    // Release tasks
    ReleasePreparation,
    ReleasePublishing,
    ReleaseNotes,
    VersionBumping,
    
    // Collaboration tasks
    CodeReview,
    PullRequest,
    IssueTracking,
    Discussion,
    
    // Infrastructure tasks
    InfrastructureAsCode,
    ConfigurationManagement,
    Monitoring,
    Logging,
    
    // Archive tasks
    LongTermArchival,
    Backup,
    DisasterRecovery,
    
    // Community tasks
    CommunityManagement,
    ContributorOnboarding,
    Outreach,
    
    // Enterprise tasks
    EnterpriseIntegration,
    CustomerDeployment,
    SLAMonitoring,
    
    // Experimental tasks
    Experimentation,
    Prototyping,
    Research,
    
    // Specialized tasks
    AutonomousAgentWork,
    CodeReviewAuthority,
    MicrosoftEnterprise,
    MinimalistUnix,
    PrivateSovereign,
    CommunityControlled,
    EthicalOSS,
}

/// Provider capability scores for different task types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapability {
    /// Provider name
    pub provider: String,
    /// Task type this capability applies to
    pub task_type: TaskType,
    /// Capability score (0.0-1.0)
    pub score: f32,
    /// Reasons for this score
    pub reasons: Vec<String>,
    /// SSH accessibility
    pub ssh_accessible: bool,
    /// Authentication methods available
    pub auth_methods: Vec<String>,
    /// Latency/availability considerations
    pub availability: f32,
    /// Cost considerations (0.0-1.0, higher is more expensive)
    pub cost: f32,
}

/// Provider matrix with comprehensive capability mapping.
pub struct ProviderMatrix {
    capabilities: Vec<ProviderCapability>,
    provider_defaults: HashMap<String, ProviderDefaults>,
}

/// Default settings for each provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefaults {
    /// Default priority for this provider
    pub default_priority: u32,
    /// Whether this provider is SSH-accessible by default
    pub ssh_accessible: bool,
    /// Default authentication method
    pub default_auth: String,
    /// Geographic regions where this provider is optimal
    pub optimal_regions: Vec<String>,
    /// Typical latency in ms
    pub typical_latency: u32,
}

impl ProviderMatrix {
    /// Create a new provider matrix with comprehensive capability mappings.
    pub fn new() -> Self {
        let capabilities = Self::build_capabilities();
        let provider_defaults = Self::build_provider_defaults();
        
        Self {
            capabilities,
            provider_defaults,
        }
    }
    
    /// Build comprehensive capability mappings for all providers.
    fn build_capabilities() -> Vec<ProviderCapability> {
        let mut capabilities = Vec::new();
        
        // GitHub capabilities
        capabilities.extend(Self::github_capabilities());
        
        // GitLab capabilities
        capabilities.extend(Self::gitlab_capabilities());
        
        // Codeberg capabilities
        capabilities.extend(Self::codeberg_capabilities());
        
        // Bitbucket capabilities
        capabilities.extend(Self::bitbucket_capabilities());
        
        // SourceHut capabilities
        capabilities.extend(Self::sourcehut_capabilities());
        
        // Gitea/Forgejo capabilities
        capabilities.extend(Self::gitea_capabilities());
        
        // Gerrit capabilities
        capabilities.extend(Self::gerrit_capabilities());
        
        // Azure DevOps capabilities
        capabilities.extend(Self::azure_capabilities());
        
        // AWS CodeCommit capabilities
        capabilities.extend(Self::aws_capabilities());
        
        // GCP capabilities
        capabilities.extend(Self::gcp_capabilities());
        
        capabilities
    }
    
    /// GitHub capability mappings.
    fn github_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::FeatureDevelopment,
                score: 0.95,
                reasons: vec![
                    "Largest developer network and community".to_string(),
                    "Excellent fork and contribution workflow".to_string(),
                    "Strong issue tracking and project management".to_string(),
                    "GitHub Actions for CI/CD integration".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::BugFix,
                score: 0.90,
                reasons: vec![
                    "Robust issue tracking with templates".to_string(),
                    "Pull request workflow with code review".to_string(),
                    "Security advisories and CVE handling".to_string(),
                    "Dependabot for automated dependency updates".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::Refactoring,
                score: 0.85,
                reasons: vec![
                    "Branch protection and workflow rules".to_string(),
                    "Pull request workflow for refactoring".to_string(),
                    "Code review support for large changes".to_string(),
                    "Issue tracking for refactoring tasks".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::Documentation,
                score: 0.95,
                reasons: vec![
                    "GitHub Pages for documentation hosting".to_string(),
                    "Wiki and integrated documentation tools".to_string(),
                    "Markdown rendering and preview".to_string(),
                    "Strong community documentation practices".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::CodeReview,
                score: 0.90,
                reasons: vec![
                    "Pull request workflow with review requests".to_string(),
                    "Protected branches and required reviews".to_string(),
                    "Code owners and file-level review assignment".to_string(),
                    "Review comments and suggestions".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::CommunityManagement,
                score: 0.95,
                reasons: vec![
                    "Discussions for community engagement".to_string(),
                    "Stars and forks as social proof".to_string(),
                    "Sponsorships for funding".to_string(),
                    "GitHub Sponsors integration".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::ReleasePublishing,
                score: 0.90,
                reasons: vec![
                    "GitHub Releases with assets".to_string(),
                    "Automatic tag creation and changelog".to_string(),
                    "Integration with package registries".to_string(),
                    "Release notes generation".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::PullRequest,
                score: 0.90,
                reasons: vec![
                    "Industry-standard pull request workflow".to_string(),
                    "Code review integration".to_string(),
                    "Discussion and collaboration features".to_string(),
                    "Integration with CI/CD".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::IssueTracking,
                score: 0.90,
                reasons: vec![
                    "Comprehensive issue tracking".to_string(),
                    "Issue templates and forms".to_string(),
                    "Project boards and milestones".to_string(),
                    "Integration with pull requests".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "github".to_string(),
                task_type: TaskType::Experimentation,
                score: 0.80,
                reasons: vec![
                    "Forking for experimentation".to_string(),
                    "GitHub Codespaces for development".to_string(),
                    "Branching strategies for experiments".to_string(),
                    "Community feedback on experiments".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.99,
                cost: 0.0,
            },
        ]
    }
    
    /// GitLab capability mappings.
    fn gitlab_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "gitlab".to_string(),
                task_type: TaskType::ContinuousIntegration,
                score: 0.95,
                reasons: vec![
                    "Native GitLab CI/CD with comprehensive pipeline support".to_string(),
                    "Docker integration and container registry".to_string(),
                    "Auto DevOps for automated workflows".to_string(),
                    "Pipeline visualization and debugging".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.98,
                cost: 0.2,
            },
            ProviderCapability {
                provider: "gitlab".to_string(),
                task_type: TaskType::SecurityScanning,
                score: 0.95,
                reasons: vec![
                    "Built-in SAST, DAST, and dependency scanning".to_string(),
                    "Container scanning and license compliance".to_string(),
                    "Security dashboards and vulnerability reports".to_string(),
                    "Automatic security remediation suggestions".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.98,
                cost: 0.2,
            },
            ProviderCapability {
                provider: "gitlab".to_string(),
                task_type: TaskType::MutationTesting,
                score: 0.90,
                reasons: vec![
                    "Custom pipeline stages for mutation testing".to_string(),
                    "Docker support for test environments".to_string(),
                    "Parallel job execution for performance".to_string(),
                    "Artifact caching and dependency management".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.98,
                cost: 0.2,
            },
            ProviderCapability {
                provider: "gitlab".to_string(),
                task_type: TaskType::ComplianceCheck,
                score: 0.90,
                reasons: vec![
                    "License compliance scanning".to_string(),
                    "Policy as code with pipeline rules".to_string(),
                    "Audit logging and compliance reports".to_string(),
                    "Enterprise-grade security controls".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.98,
                cost: 0.2,
            },
            ProviderCapability {
                provider: "gitlab".to_string(),
                task_type: TaskType::InfrastructureAsCode,
                score: 0.85,
                reasons: vec![
                    "GitLab Terraform integration".to_string(),
                    "Infrastructure pipeline support".to_string(),
                    "Environment management and promotion".to_string(),
                    "Kubernetes integration and deployment".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.98,
                cost: 0.2,
            },
        ]
    }
    
    /// Codeberg capability mappings.
    fn codeberg_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "codeberg".to_string(),
                task_type: TaskType::LongTermArchival,
                score: 0.90,
                reasons: vec![
                    "Non-profit, community-owned infrastructure".to_string(),
                    "No corporate ownership or acquisition risk".to_string(),
                    "Privacy-focused and GDPR compliant".to_string(),
                    "Sustainable funding model".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.95,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "codeberg".to_string(),
                task_type: TaskType::CommunityManagement,
                score: 0.85,
                reasons: vec![
                    "Community-driven governance".to_string(),
                    "Open-source focused community".to_string(),
                    "Ethical alternative to corporate platforms".to_string(),
                    "Federation with other Forgejo instances".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.95,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "codeberg".to_string(),
                task_type: TaskType::Backup,
                score: 0.90,
                reasons: vec![
                    "Independent backup location".to_string(),
                    "Different legal jurisdiction (Germany)".to_string(),
                    "No single point of failure with GitHub".to_string(),
                    "Mirror support for redundancy".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.95,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "codeberg".to_string(),
                task_type: TaskType::Documentation,
                score: 0.80,
                reasons: vec![
                    "Codeberg Pages for documentation".to_string(),
                    "Markdown and Wiki support".to_string(),
                    "Issue tracking for documentation".to_string(),
                    "Lightweight and fast".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.95,
                cost: 0.0,
            },
        ]
    }
    
    /// Bitbucket capability mappings.
    fn bitbucket_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "bitbucket".to_string(),
                task_type: TaskType::EnterpriseIntegration,
                score: 0.90,
                reasons: vec![
                    "Deep Jira integration for issue tracking".to_string(),
                    "Confluence integration for documentation".to_string(),
                    "Atlassian ecosystem synergy".to_string(),
                    "Enterprise SSO and user management".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "atlassian".to_string()],
                availability: 0.97,
                cost: 0.5,
            },
            ProviderCapability {
                provider: "bitbucket".to_string(),
                task_type: TaskType::CodeReview,
                score: 0.85,
                reasons: vec![
                    "Pull request workflow with Jira integration".to_string(),
                    "Code review approvals and restrictions".to_string(),
                    "Branch permissions and workflows".to_string(),
                    "Bitbucket Pipelines for CI/CD".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "atlassian".to_string()],
                availability: 0.97,
                cost: 0.5,
            },
            ProviderCapability {
                provider: "bitbucket".to_string(),
                task_type: TaskType::ContinuousIntegration,
                score: 0.80,
                reasons: vec![
                    "Bitbucket Pipelines with Docker support".to_string(),
                    "Integration with Jira for deployment tracking".to_string(),
                    "Deployment pipelines to various environments".to_string(),
                    "Built-in test reporting".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "atlassian".to_string()],
                availability: 0.97,
                cost: 0.5,
            },
        ]
    }
    
    /// SourceHut capability mappings.
    fn sourcehut_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "sourcehut".to_string(),
                task_type: TaskType::MinimalistUnix,
                score: 0.95,
                reasons: vec![
                    "Email-based workflow (no JavaScript)".to_string(),
                    "Unix philosophy and minimal dependencies".to_string(),
                    "SSH-first development workflow".to_string(),
                    "Lightweight and fast".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.92,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "sourcehut".to_string(),
                task_type: TaskType::Documentation,
                score: 0.85,
                reasons: vec![
                    "Man pages and markdown support".to_string(),
                    "Wiki functionality".to_string(),
                    "Ticket tracking for documentation issues".to_string(),
                    "Email-based documentation workflow".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.92,
                cost: 0.0,
            },
            ProviderCapability {
                provider: "sourcehut".to_string(),
                task_type: TaskType::ContinuousIntegration,
                score: 0.80,
                reasons: vec![
                    "SourceHut builds with multiple platform support".to_string(),
                    "Email notifications for build status".to_string(),
                    "Simple and transparent CI system".to_string(),
                    "No vendor lock-in".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.92,
                cost: 0.0,
            },
        ]
    }
    
    /// Gitea/Forgejo capability mappings.
    fn gitea_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "gitea".to_string(),
                task_type: TaskType::PrivateSovereign,
                score: 0.95,
                reasons: vec![
                    "Self-hosted with full control".to_string(),
                    "Lightweight resource requirements".to_string(),
                    "GitHub-compatible API".to_string(),
                    "Easy deployment and maintenance".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.90,
                cost: 0.1,
            },
            ProviderCapability {
                provider: "forgejo".to_string(),
                task_type: TaskType::CommunityControlled,
                score: 0.90,
                reasons: vec![
                    "Community-governed fork of Gitea".to_string(),
                    "Focus on governance and community ownership".to_string(),
                    "Federation support".to_string(),
                    "Open-source community focus".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.90,
                cost: 0.1,
            },
            ProviderCapability {
                provider: "gitea".to_string(),
                task_type: TaskType::AutonomousAgentWork,
                score: 0.85,
                reasons: vec![
                    "API-friendly for automation".to_string(),
                    "Webhook support for CI/CD integration".to_string(),
                    "Lightweight for agent deployments".to_string(),
                    "Easy containerization".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "token".to_string()],
                availability: 0.90,
                cost: 0.1,
            },
        ]
    }
    
    /// Gerrit capability mappings.
    fn gerrit_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "gerrit".to_string(),
                task_type: TaskType::CodeReviewAuthority,
                score: 0.95,
                reasons: vec![
                    "Strict code review gates".to_string(),
                    "Change-Id based workflow".to_string(),
                    "Access control and permissions".to_string(),
                    "Used by large engineering organizations".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.93,
                cost: 0.3,
            },
            ProviderCapability {
                provider: "gerrit".to_string(),
                task_type: TaskType::SecurityAudit,
                score: 0.85,
                reasons: vec![
                    "Strict access controls".to_string(),
                    "Detailed review history".to_string(),
                    "Integration with enterprise authentication".to_string(),
                    "Audit trail for all changes".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string()],
                availability: 0.93,
                cost: 0.3,
            },
        ]
    }
    
    /// Azure DevOps capability mappings.
    fn azure_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "azure".to_string(),
                task_type: TaskType::MicrosoftEnterprise,
                score: 0.95,
                reasons: vec![
                    "Deep Azure ecosystem integration".to_string(),
                    "Enterprise-grade security and compliance".to_string(),
                    "Azure Active Directory integration".to_string(),
                    "Windows/.NET ecosystem support".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "azure".to_string()],
                availability: 0.98,
                cost: 0.6,
            },
            ProviderCapability {
                provider: "azure".to_string(),
                task_type: TaskType::ContinuousDeployment,
                score: 0.90,
                reasons: vec![
                    "Azure Pipelines with multi-cloud support".to_string(),
                    "Integration with Azure services".to_string(),
                    "Enterprise release management".to_string(),
                    "Azure DevOps Test Plans".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "azure".to_string()],
                availability: 0.98,
                cost: 0.6,
            },
            ProviderCapability {
                provider: "azure".to_string(),
                task_type: TaskType::EnterpriseIntegration,
                score: 0.90,
                reasons: vec![
                    "Microsoft 365 integration".to_string(),
                    "Power Platform integration".to_string(),
                    "Enterprise templates and governance".to_string(),
                    "Hybrid cloud support".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "azure".to_string()],
                availability: 0.98,
                cost: 0.6,
            },
        ]
    }
    
    /// AWS CodeCommit capability mappings.
    fn aws_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "aws".to_string(),
                task_type: TaskType::InfrastructureAsCode,
                score: 0.90,
                reasons: vec![
                    "Deep AWS IAM integration".to_string(),
                    "VPC and security group integration".to_string(),
                    "CloudFormation and Terraform support".to_string(),
                    "AWS-native authentication".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "iam".to_string()],
                availability: 0.99,
                cost: 0.4,
            },
            ProviderCapability {
                provider: "aws".to_string(),
                task_type: TaskType::ConfigurationManagement,
                score: 0.85,
                reasons: vec![
                    "AWS Systems Manager integration".to_string(),
                    "CloudWatch monitoring integration".to_string(),
                    "Lambda-based automation".to_string(),
                    "Secrets Manager integration".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "iam".to_string()],
                availability: 0.99,
                cost: 0.4,
            },
            ProviderCapability {
                provider: "aws".to_string(),
                task_type: TaskType::Backup,
                score: 0.85,
                reasons: vec![
                    "S3 and Glacier integration for backup".to_string(),
                    "Cross-region replication".to_string(),
                    "AWS Backup service integration".to_string(),
                    "Compliance and retention policies".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "iam".to_string()],
                availability: 0.99,
                cost: 0.4,
            },
        ]
    }
    
    /// GCP Source Repositories capability mappings.
    fn gcp_capabilities() -> Vec<ProviderCapability> {
        vec![
            ProviderCapability {
                provider: "gcp".to_string(),
                task_type: TaskType::ContinuousIntegration,
                score: 0.85,
                reasons: vec![
                    "Cloud Build integration".to_string(),
                    "Google Kubernetes Engine integration".to_string(),
                    "Cloud Run and serverless support".to_string(),
                    "Anthos hybrid cloud support".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "oauth".to_string()],
                availability: 0.98,
                cost: 0.5,
            },
            ProviderCapability {
                provider: "gcp".to_string(),
                task_type: TaskType::InfrastructureAsCode,
                score: 0.80,
                reasons: vec![
                    "Google Cloud Deployment Manager".to_string(),
                    "Terraform GCP provider integration".to_string(),
                    "Google Cloud IAM integration".to_string(),
                    "VPC and networking integration".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "oauth".to_string()],
                availability: 0.98,
                cost: 0.5,
            },
            ProviderCapability {
                provider: "gcp".to_string(),
                task_type: TaskType::Monitoring,
                score: 0.85,
                reasons: vec![
                    "Cloud Monitoring integration".to_string(),
                    "Cloud Logging integration".to_string(),
                    "Stackdriver integration".to_string(),
                    "Alerting and incident response".to_string(),
                ],
                ssh_accessible: true,
                auth_methods: vec!["ssh".to_string(), "https".to_string(), "oauth".to_string()],
                availability: 0.98,
                cost: 0.5,
            },
        ]
    }
    
    /// Build provider defaults.
    fn build_provider_defaults() -> HashMap<String, ProviderDefaults> {
        let mut defaults = HashMap::new();
        
        defaults.insert("github".to_string(), ProviderDefaults {
            default_priority: 10,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["global".to_string(), "us-east".to_string(), "eu-west".to_string()],
            typical_latency: 50,
        });
        
        defaults.insert("gitlab".to_string(), ProviderDefaults {
            default_priority: 20,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["global".to_string(), "eu-central".to_string(), "us-central".to_string()],
            typical_latency: 60,
        });
        
        defaults.insert("codeberg".to_string(), ProviderDefaults {
            default_priority: 30,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["eu-central".to_string(), "global".to_string()],
            typical_latency: 80,
        });
        
        defaults.insert("bitbucket".to_string(), ProviderDefaults {
            default_priority: 40,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["global".to_string(), "us-west".to_string(), "ap-southeast".to_string()],
            typical_latency: 70,
        });
        
        defaults.insert("sourcehut".to_string(), ProviderDefaults {
            default_priority: 50,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["global".to_string()],
            typical_latency: 100,
        });
        
        defaults.insert("gitea".to_string(), ProviderDefaults {
            default_priority: 60,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["self-hosted".to_string()],
            typical_latency: 20,
        });
        
        defaults.insert("forgejo".to_string(), ProviderDefaults {
            default_priority: 65,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["self-hosted".to_string(), "eu-central".to_string()],
            typical_latency: 25,
        });
        
        defaults.insert("gerrit".to_string(), ProviderDefaults {
            default_priority: 70,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["enterprise".to_string()],
            typical_latency: 30,
        });
        
        defaults.insert("azure".to_string(), ProviderDefaults {
            default_priority: 80,
            ssh_accessible: true,
            default_auth: "ssh".to_string(),
            optimal_regions: vec!["enterprise".to_string(), "microsoft".to_string()],
            typical_latency: 40,
        });
        
        defaults.insert("aws".to_string(), ProviderDefaults {
            default_priority: 85,
            ssh_accessible: true,
            default_auth: "iam".to_string(),
            optimal_regions: vec!["aws-global".to_string(), "us-east".to_string(), "eu-west".to_string()],
            typical_latency: 45,
        });
        
        defaults.insert("gcp".to_string(), ProviderDefaults {
            default_priority: 90,
            ssh_accessible: true,
            default_auth: "oauth".to_string(),
            optimal_regions: vec!["gcp-global".to_string(), "us-central".to_string(), "europe-west".to_string()],
            typical_latency: 50,
        });
        
        defaults
    }
    
    /// Get best providers for a specific task type.
    pub fn get_best_providers(&self, task_type: &TaskType, ssh_only: bool) -> Vec<ProviderCapability> {
        let mut matching: Vec<_> = self.capabilities
            .iter()
            .filter(|cap| &cap.task_type == task_type)
            .filter(|cap| !ssh_only || cap.ssh_accessible)
            .cloned()
            .collect();
        
        // Sort by score (descending), then by availability (descending), then by cost (ascending)
        matching.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap()
                .then_with(|| b.availability.partial_cmp(&a.availability).unwrap())
                .then_with(|| a.cost.partial_cmp(&b.cost).unwrap())
        });
        
        matching
    }
    
    /// Get all SSH-accessible providers.
    pub fn get_ssh_providers(&self) -> HashSet<String> {
        self.capabilities
            .iter()
            .filter(|cap| cap.ssh_accessible)
            .map(|cap| cap.provider.clone())
            .collect()
    }
    
    /// Get provider defaults.
    pub fn get_provider_defaults(&self, provider: &str) -> Option<&ProviderDefaults> {
        self.provider_defaults.get(provider)
    }
    
    /// Calculate fit score for a provider-task combination.
    pub fn calculate_fit_score(&self, provider: &str, task_type: &TaskType) -> f32 {
        let capability = self.capabilities
            .iter()
            .find(|cap| cap.provider == provider && cap.task_type == *task_type);
        
        match capability {
            Some(cap) => {
                // Weighted score: capability (70%) + availability (20%) + cost inverse (10%)
                let cost_score = 1.0 - cap.cost;
                cap.score * 0.7 + cap.availability * 0.2 + cost_score * 0.1
            }
            None => 0.0,
        }
    }
    
    /// Recommend providers for a task with fallback chain.
    pub fn recommend_providers_with_fallback(
        &self,
        task_type: &TaskType,
        ssh_only: bool,
        count: usize,
    ) -> Vec<String> {
        let best_providers = self.get_best_providers(task_type, ssh_only);
        best_providers
            .into_iter()
            .take(count)
            .map(|cap| cap.provider)
            .collect()
    }
    
    /// Validate that configured remotes are SSH-accessible.
    pub fn validate_ssh_access(&self, remotes: &[RemoteConfig]) -> Result<()> {
        let ssh_providers = self.get_ssh_providers();
        
        for remote in remotes {
            if !ssh_providers.contains(&remote.provider) {
                tracing::warn!(
                    provider = %remote.provider,
                    "provider is not SSH-accessible according to provider matrix"
                );
            }
        }
        
        Ok(())
    }
}

impl Default for ProviderMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_best_providers_for_feature_development() {
        let matrix = ProviderMatrix::new();
        let providers = matrix.get_best_providers(&TaskType::FeatureDevelopment, true);
        
        assert!(!providers.is_empty());
        assert_eq!(providers[0].provider, "github");
        assert!(providers[0].score > 0.9);
    }
    
    #[test]
    fn test_get_best_providers_for_ci() {
        let matrix = ProviderMatrix::new();
        let providers = matrix.get_best_providers(&TaskType::ContinuousIntegration, true);
        
        assert!(!providers.is_empty());
        assert_eq!(providers[0].provider, "gitlab");
        assert!(providers[0].score > 0.9);
    }
    
    #[test]
    fn test_ssh_only_filtering() {
        let matrix = ProviderMatrix::new();
        let all_providers = matrix.get_best_providers(&TaskType::FeatureDevelopment, false);
        let ssh_providers = matrix.get_best_providers(&TaskType::FeatureDevelopment, true);
        
        // SSH-only should be subset of all providers
        assert!(ssh_providers.len() <= all_providers.len());
    }
    
    #[test]
    fn test_fit_score_calculation() {
        let matrix = ProviderMatrix::new();
        let score = matrix.calculate_fit_score("github", &TaskType::FeatureDevelopment);
        
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }
    
    #[test]
    fn test_recommendation_with_fallback() {
        let matrix = ProviderMatrix::new();
        let recommendations = matrix.recommend_providers_with_fallback(
            &TaskType::SecurityScanning,
            true,
            3,
        );
        
        assert_eq!(recommendations.len(), 3);
        assert_eq!(recommendations[0], "gitlab");
    }
    
    #[test]
    fn test_provider_defaults() {
        let matrix = ProviderMatrix::new();
        let github_defaults = matrix.get_provider_defaults("github");
        
        assert!(github_defaults.is_some());
        assert_eq!(github_defaults.unwrap().default_priority, 10);
        assert!(github_defaults.unwrap().ssh_accessible);
    }
}
