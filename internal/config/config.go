package config

import (
	"os"

	"gopkg.in/yaml.v3"
)

type Config struct {
	SIP struct {
		Transport  string `yaml:"transport"`
		ListenIp   string `yaml:"listen_ip"`
		ListenPort int    `yaml:"listen_port"`
	} `yaml:"sip"`
	Registrar struct {
		Addr    string `yaml:"addr"`
		Domain  string `yaml:"domain"`
		Realm   string `yaml:"realm"`
		Expires int64  `yaml:"expires"`
	} `yaml:"registrar"`
	Agent struct {
		StartExt int    `yaml:"start_ext"`
		Count    int    `yaml:"count"`
		Password string `yaml:"password"`
		Username string `yaml:"user_name"`
	} `yaml:"agents"`
}

func Load(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var cfg Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, err
	}
	return &cfg, nil
}
