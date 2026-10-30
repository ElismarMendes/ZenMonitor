import streamlit as st
import requests
import time

# Configuração da página
st.set_page_config(
    page_title="Zen Monitor Dashboard",
    page_icon="⚡",
    layout="wide"
)

API_URL = "http://127.0.0.1:8000/api/v1"

st.title("⚡ Painel de Monitoramento Zen - MU Online")
st.markdown("---")

# Função para buscar clientes na API
def buscar_clientes():
    try:
        response = requests.get(f"{API_URL}/clientes", timeout=2)
        if response.status_code == 200:
            return response.json().get("clientes", [])
    except Exception:
        pass
    return []

# Função para buscar o Zen de um PID específico
def buscar_zen(pid):
    try:
        response = requests.get(f"{API_URL}/zen/{pid}", timeout=2)
        if response.status_code == 200:
            return response.json()
    except Exception:
        pass
    return None

# Sidebar com controles
st.sidebar.header("⚙️ Configurações")
auto_refresh = st.sidebar.checkbox("Ativar Auto-Refresh (5s)", value=True)
refresh_counter = st.sidebar.empty()

# Layout Principal
col1, col2 = st.columns([2, 1])

with col1:
    st.subheader("📊 Contas Conectadas")
    
    clientes = buscar_clientes()
    
    if not clientes:
        st.warning("⚠️ Nenhum cliente do MU Online detectado ou a API está offline.")
        st.info("Certifique-se de que o arquivo `main.py` (a API) está rodando no terminal.")
    else:
        # Exibe as contas em cards/tabela organizada
        for cli in clientes:
            pid = cli["pid"]
            nome = cli["name"]
            res = cli["resolucao"]
            
            # Busca os dados detalhados (Zen) de cada conta
            dados_zen = buscar_zen(pid)
            zen_val = dados_zen.get("zen") if dados_zen else None
            
            # Formatação do Zen
            if zen_val is not None:
                zen_str = f"{zen_val:,}".replace(",", ".")
                alerta_limite = " ⚠️ [Limite 2B!]" if zen_val >= 1900000000 else ""
            else:
                zen_str = "--- (Aguardando leitura)"
                alerta_limite = ""

            # Card visual para cada conta
            with st.container():
                st.markdown(
                    f"""
                    <div style="background-color: #1e1e1e; padding: 15px; border-radius: 8px; margin-bottom: 10px; border-left: 5px solid #00acc1;">
                        <h4 style="margin: 0; color: #00acc1;">👤 {nome} <span style="font-size: 12px; color: #aaa;">(PID: {pid} | Res: {res})</span></h4>
                        <p style="margin: 5px 0 0 0; font-size: 16px; color: #fff;">💰 Zen: <b>{zen_str}</b> <span style="color: #ff9800;">{alerta_limite}</span></p>
                    </div>
                    """,
                    unsafe_allow_html=True
                )

with col2:
    st.subheader("💡 Status do Sistema")
    st.success("API Conectada com Sucesso")
    st.metric(label="Contas Ativas", value=len(clientes) if 'clientes' in locals() else 0)
    
    st.markdown("---")
    st.markdown("### 🛠️ Próximas Funções")
    st.markdown("- Integração com Alertas (Discord/Telegram)")
    st.markdown("- Comandos do Helper em Background")

# Controle de Atualização Automática
if auto_refresh:
    time.sleep(5)
    st.rerun()