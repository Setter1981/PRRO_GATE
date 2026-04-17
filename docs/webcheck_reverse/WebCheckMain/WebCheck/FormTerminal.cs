using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO.Ports;
using System.Net;
using System.Net.Sockets;
using System.Runtime.CompilerServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Newtonsoft.Json.Linq;

namespace WebCheck;

[DesignerGenerated]
public class FormTerminal : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Timer1")]
	private Timer _Timer1;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonClose")]
	private Button _ButtonClose;

	[CompilerGenerated]
	[AccessedThroughProperty("Timer2")]
	private Timer _Timer2;

	private TypCardserv RRR;

	private SerialPort Port;

	private Socket Sock;

	private TcpClient ClientSocket;

	private NetworkStream ServerStream;

	private int Tim;

	private int stZ;

	private const int IntervalT = 500;

	private int LimitTimeUp;

	private const string NameFormT = "Оплата карткою                                                  ";

	private HexTextBpos BP;

	private string BposTS;

	private string sRbpos;

	internal virtual Timer Timer1
	{
		[CompilerGenerated]
		get
		{
			return _Timer1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Timer1_Tick;
			Timer timer = _Timer1;
			if (timer != null)
			{
				timer.Tick -= eventHandler;
			}
			_Timer1 = value;
			timer = _Timer1;
			if (timer != null)
			{
				timer.Tick += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxStatus")]
	internal virtual TextBox TextBoxStatus
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ButtonClose
	{
		[CompilerGenerated]
		get
		{
			return _ButtonClose;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ButtonClose_Click;
			Button buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				((Control)buttonClose).Click -= eventHandler;
			}
			_ButtonClose = value;
			buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				((Control)buttonClose).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxSum")]
	internal virtual TextBox TextBoxSum
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBoxOrder")]
	internal virtual TextBox TextBoxOrder
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PictureBox1")]
	internal virtual PictureBox PictureBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Timer Timer2
	{
		[CompilerGenerated]
		get
		{
			return _Timer2;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Timer2_Tick;
			Timer timer = _Timer2;
			if (timer != null)
			{
				timer.Tick -= eventHandler;
			}
			_Timer2 = value;
			timer = _Timer2;
			if (timer != null)
			{
				timer.Tick += eventHandler;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0022: Unknown result type (might be due to invalid IL or missing references)
		//IL_002c: Expected O, but got Unknown
		//IL_002d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0037: Expected O, but got Unknown
		//IL_0038: Unknown result type (might be due to invalid IL or missing references)
		//IL_0042: Expected O, but got Unknown
		//IL_0043: Unknown result type (might be due to invalid IL or missing references)
		//IL_004d: Expected O, but got Unknown
		//IL_004e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0058: Expected O, but got Unknown
		//IL_0059: Unknown result type (might be due to invalid IL or missing references)
		//IL_0063: Expected O, but got Unknown
		//IL_0064: Unknown result type (might be due to invalid IL or missing references)
		//IL_006e: Expected O, but got Unknown
		//IL_006f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0079: Expected O, but got Unknown
		//IL_0080: Unknown result type (might be due to invalid IL or missing references)
		//IL_008a: Expected O, but got Unknown
		//IL_00b2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00bc: Expected O, but got Unknown
		//IL_015c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0166: Expected O, but got Unknown
		//IL_01ef: Unknown result type (might be due to invalid IL or missing references)
		//IL_01f9: Expected O, but got Unknown
		//IL_028a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0294: Expected O, but got Unknown
		//IL_0310: Unknown result type (might be due to invalid IL or missing references)
		//IL_031a: Expected O, but got Unknown
		//IL_039d: Unknown result type (might be due to invalid IL or missing references)
		//IL_03a7: Expected O, but got Unknown
		//IL_0425: Unknown result type (might be due to invalid IL or missing references)
		//IL_042f: Expected O, but got Unknown
		//IL_0545: Unknown result type (might be due to invalid IL or missing references)
		//IL_054f: Expected O, but got Unknown
		components = new Container();
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTerminal));
		Timer1 = new Timer(components);
		TextBoxStatus = new TextBox();
		ButtonClose = new Button();
		TextBoxSum = new TextBox();
		TextBoxOrder = new TextBox();
		PictureBox1 = new PictureBox();
		Label1 = new Label();
		Label2 = new Label();
		Timer2 = new Timer(components);
		((ISupportInitialize)PictureBox1).BeginInit();
		((Control)this).SuspendLayout();
		((Control)TextBoxStatus).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxStatus).Location = new Point(12, 205);
		TextBoxStatus.Multiline = true;
		((Control)TextBoxStatus).Name = "TextBoxStatus";
		((TextBoxBase)TextBoxStatus).ReadOnly = true;
		((Control)TextBoxStatus).Size = new Size(753, 197);
		((Control)TextBoxStatus).TabIndex = 5;
		((Control)TextBoxStatus).TabStop = false;
		TextBoxStatus.TextAlign = (HorizontalAlignment)2;
		((Control)ButtonClose).Anchor = (AnchorStyles)6;
		((Control)ButtonClose).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ButtonClose).Location = new Point(12, 423);
		((Control)ButtonClose).Name = "ButtonClose";
		((Control)ButtonClose).Size = new Size(753, 62);
		((Control)ButtonClose).TabIndex = 6;
		((Control)ButtonClose).TabStop = false;
		((ButtonBase)ButtonClose).Text = "ЗАКРИТИ";
		((ButtonBase)ButtonClose).UseVisualStyleBackColor = true;
		((Control)TextBoxSum).Font = new Font("Consolas", 19.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxSum).Location = new Point(409, 118);
		TextBoxSum.Multiline = true;
		((Control)TextBoxSum).Name = "TextBoxSum";
		((TextBoxBase)TextBoxSum).ReadOnly = true;
		((Control)TextBoxSum).Size = new Size(356, 43);
		((Control)TextBoxSum).TabIndex = 7;
		((Control)TextBoxSum).TabStop = false;
		TextBoxSum.TextAlign = (HorizontalAlignment)1;
		((Control)TextBoxOrder).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxOrder).Location = new Point(12, 119);
		TextBoxOrder.Multiline = true;
		((Control)TextBoxOrder).Name = "TextBoxOrder";
		((TextBoxBase)TextBoxOrder).ReadOnly = true;
		((Control)TextBoxOrder).Size = new Size(356, 44);
		((Control)TextBoxOrder).TabIndex = 8;
		((Control)TextBoxOrder).TabStop = false;
		PictureBox1.Image = (Image)componentResourceManager.GetObject("PictureBox1.Image");
		((Control)PictureBox1).Location = new Point(12, 12);
		((Control)PictureBox1).Name = "PictureBox1";
		((Control)PictureBox1).Size = new Size(305, 101);
		PictureBox1.SizeMode = (PictureBoxSizeMode)4;
		PictureBox1.TabIndex = 9;
		PictureBox1.TabStop = false;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(323, 50);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(436, 29);
		((Control)Label1).TabIndex = 10;
		Label1.Text = "Операція з банківським терміналом";
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(12, 177);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(165, 25);
		((Control)Label2).TabIndex = 11;
		Label2.Text = "Статус операції:";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(780, 497);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)PictureBox1);
		((Control)this).Controls.Add((Control)(object)TextBoxOrder);
		((Control)this).Controls.Add((Control)(object)TextBoxSum);
		((Control)this).Controls.Add((Control)(object)ButtonClose);
		((Control)this).Controls.Add((Control)(object)TextBoxStatus);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTerminal";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Оплата карткою";
		((Form)this).TopMost = true;
		((ISupportInitialize)PictureBox1).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormTerminal_Closing(object sender, CancelEventArgs e)
	{
		//IL_0025: Unknown result type (might be due to invalid IL or missing references)
		//IL_002b: Invalid comparison between Unknown and I4
		if ((Timer1.Enabled & ((Control)ButtonClose).Enabled) && (int)Interaction.MsgBox((object)"Вікно буде закрито. Термінал продовжить працювати без керування програмою. Перевірте що на терміналі не виконується трансакція.", (MsgBoxStyle)49, (object)"Увага") == 2)
		{
			e.Cancel = true;
			return;
		}
		e.Cancel = !((Control)ButtonClose).Enabled;
		if (!((Control)ButtonClose).Enabled)
		{
			return;
		}
		if ((!All.CardservTrue & (All.B.CurrentStatus.Length <= 27)) && Operators.CompareString(All.B.CurrentStatus.Substring(0, 5), "LastM", false) == 0)
		{
			All.B.CurrentStatus += "_error=true_errordescription=Неможливо отримати данні від термінала або можливо робота драйвера була зупинена аварійно по таймауту або дією касира.";
		}
		switch (RRR.Connect)
		{
		case 1:
			try
			{
				Port.Close();
				_ = null;
				break;
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				All.LgT.SaveTextToLogCardserv("COM port ERROR: ", ex4.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		case 3:
			try
			{
				Port.Close();
				_ = null;
				break;
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				All.LgT.SaveTextToLogCardserv("COM port ERROR: ", ex6.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		case 2:
			try
			{
				Sock.Close();
				break;
			}
			catch (Exception ex11)
			{
				ProjectData.SetProjectError(ex11);
				Exception ex12 = ex11;
				All.LgT.SaveTextToLogCardserv("IP port ERROR: ", ex12.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		case 4:
			try
			{
				Sock.Close();
				break;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				All.LgT.SaveTextToLogCardserv("IP port ERROR: ", ex10.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		case 5:
			try
			{
				Port.Close();
				_ = null;
				break;
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				All.LgT.SaveTextToLogCardserv("COM port ERROR: ", ex8.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		case 6:
		{
			string text = StatusPort(OnlyReadBPOS: true);
			if (text.Length > 0)
			{
				All.LgT.SaveTextToLogCardserv("REMAINING CLIPBOARD", text);
			}
			try
			{
				ServerStream.Close();
				ClientSocket.Close();
				break;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.LgT.SaveTextToLogCardserv("IP port ERROR: ", ex2.ToString());
				ProjectData.ClearProjectError();
				break;
			}
		}
		}
	}

	public FormTerminal(TypCardserv e)
	{
		//IL_002b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0035: Expected O, but got Unknown
		((Form)this).Closing += FormTerminal_Closing;
		((Form)this).Load += FormTerminal_Load;
		Port = new SerialPort();
		Sock = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
		ClientSocket = new TcpClient();
		Tim = 0;
		stZ = 1;
		LimitTimeUp = 35000;
		BP = new HexTextBpos();
		BposTS = "";
		sRbpos = "";
		InitializeComponent();
		RRR = e;
	}

	private void FormTerminal_Load(object sender, EventArgs e)
	{
		checked
		{
			LimitTimeUp = (int)Math.Round((double)All.f.GetInteger("Global", "TerminalTimeout", 0) / 2.0 * 1000.0);
			if (LimitTimeUp < 9000)
			{
				LimitTimeUp = 35000;
				All.f.WriteInteger("Global", "TerminalTimeout", (int)Math.Round((double)(LimitTimeUp * 2) / 1000.0));
			}
			All.B.CurrentStatus = "LastMethod=" + RRR.method;
			((Form)this).AcceptButton = (IButtonControl)(object)ButtonClose;
			((Control)ButtonClose).Enabled = false;
			Tim = 0;
			((Form)this).Text = "Оплата карткою                                                  ";
			Timer1.Interval = 500;
			TextBoxOrder.Text = All.MethodEnToUa(RRR.method).ToUpper();
			TextBoxSum.Text = RRR.amount;
			if (RRR.Connect == 999)
			{
				((Form)this).Text = "Оплата карткою                                                  DEMO";
				TextBoxStatus.Text = "Працює демонстраційний режим для розробників...";
				Demo();
				Timer1.Stop();
				((Control)ButtonClose).Enabled = true;
				All.CardservTrue = true;
			}
			else if (SettingsTerminal())
			{
				if (!StartMethod())
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					All.CardservTrue = false;
					return;
				}
				int num = All.f.GetInteger("Global", "TerminalTimeoutUnlockBut", -1) * 1000;
				if (num < 0)
				{
					num = 0;
					All.f.WriteInteger("Global", "TerminalTimeoutUnlockBut", num);
				}
				if (num > 0)
				{
					Timer2.Interval = num;
					Timer2.Start();
				}
			}
			else
			{
				Timer1.Stop();
				((Control)ButtonClose).Enabled = true;
				All.CardservTrue = false;
			}
		}
	}

	private bool SettingsTerminal()
	{
		bool result;
		switch (RRR.Connect)
		{
		case 1:
			try
			{
				SerialPort port3 = Port;
				port3.PortName = RRR.dest;
				port3.BaudRate = 115200;
				port3.DataBits = 8;
				port3.Parity = (Parity)0;
				port3.StopBits = (StopBits)1;
				port3.ReadTimeout = 10000;
				port3.WriteTimeout = 10000;
				port3.Open();
				_ = null;
				string text4 = StatusPort();
				if (text4.Length > 0)
				{
					All.LgT.SaveTextToLogCardserv("REMAINING CLIPBOARD", text4);
					TextBoxStatus.Text = "- Перевірте банківський термінал та завершить поточну операцію на ньому." + Environment.NewLine + "- Натисніть 'ЗАКРИТИ' та повторіть операцію.";
					((Control)ButtonClose).Enabled = true;
					result = false;
					break;
				}
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Очікуються данні від банківського термінала (com port opened)";
				stZ = MethodEnStZ();
				goto default;
			}
			catch (Exception ex11)
			{
				ProjectData.SetProjectError(ex11);
				Exception ex12 = ex11;
				All.B.CurrentStatus += "_Error=Помилка відкриття COM порту";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex12.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
			}
			break;
		case 3:
			try
			{
				SerialPort port2 = Port;
				port2.PortName = RRR.dest;
				port2.BaudRate = 9600;
				port2.DataBits = 8;
				port2.Parity = (Parity)0;
				port2.StopBits = (StopBits)1;
				port2.ReadTimeout = 10000;
				port2.WriteTimeout = 10000;
				port2.Open();
				_ = null;
				string text3 = StatusPort();
				if (text3.Length > 0)
				{
					All.LgT.SaveTextToLogCardserv("REMAINING CLIPBOARD", text3);
					TextBoxStatus.Text = "- Перевірте банківський термінал та завершить поточну операцію на ньому." + Environment.NewLine + "- Натисніть 'ЗАКРИТИ' та повторіть операцію.";
					((Control)ButtonClose).Enabled = true;
					result = false;
					break;
				}
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Виконується операція на банківському терміналі...";
				stZ = MethodEnStZ();
				goto default;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				All.B.CurrentStatus += "_Error=Помилка відкриття COM порту";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex10.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
			}
			break;
		case 2:
			try
			{
				Socket sock2 = Sock;
				sock2.SendTimeout = 5000;
				sock2.ReceiveTimeout = 18;
				sock2.Connect(new IPEndPoint(IPAddress.Parse(RRR.dest), RRR.port));
				_ = null;
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Очікуються данні від банківського термінала (ip connect)";
				stZ = MethodEnStZ();
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				All.B.CurrentStatus += "_Error=Помилка підключення IP";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex8.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
				break;
			}
			goto default;
		case 4:
			try
			{
				Socket sock = Sock;
				sock.SendTimeout = 5000;
				sock.ReceiveTimeout = 18;
				sock.Connect(new IPEndPoint(IPAddress.Parse(RRR.dest), RRR.port));
				_ = null;
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Очікуються данні від банківського термінала (ip connect)";
				stZ = MethodEnStZ();
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				All.B.CurrentStatus += "_Error=Помилка підключення IP";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex6.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
				break;
			}
			goto default;
		case 5:
			try
			{
				SerialPort port = Port;
				port.PortName = RRR.dest;
				port.BaudRate = 9600;
				port.DataBits = 8;
				port.Parity = (Parity)0;
				port.StopBits = (StopBits)1;
				port.ReadTimeout = 10000;
				port.WriteTimeout = 10000;
				port.Open();
				_ = null;
				string text2 = StatusPort(OnlyReadBPOS: true);
				if (text2.Length > 0)
				{
					All.LgT.SaveTextToLogCardserv("REMAINING CLIPBOARD", text2);
					TextBoxStatus.Text = "- Перевірте банківський термінал та завершить поточну операцію на ньому." + Environment.NewLine + "- Натисніть 'ЗАКРИТИ' та повторіть операцію.";
					((Control)ButtonClose).Enabled = true;
					result = false;
					break;
				}
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Виконується операція на банківському терміналі...";
				stZ = MethodEnStZ();
				goto default;
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				All.B.CurrentStatus += "_Error=Помилка відкриття COM порту";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex4.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
			}
			break;
		case 6:
			try
			{
				ClientSocket.SendTimeout = 5000;
				ClientSocket.ReceiveTimeout = 18;
				ClientSocket.Connect(IPAddress.Parse(RRR.dest), RRR.port);
				ServerStream = ClientSocket.GetStream();
				string text = StatusPort(OnlyReadBPOS: true);
				if (text.Length > 0)
				{
					All.LgT.SaveTextToLogCardserv("REMAINING CLIPBOARD", text);
				}
				((Form)this).Text = "Оплата карткою                                                  " + Tim / 1000;
				TextBoxStatus.Text = "Очікуються данні від банківського термінала (ip connect)";
				stZ = MethodEnStZ();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.B.CurrentStatus += "_Error=Помилка підключення IP";
				TextBoxStatus.Text = "Помилка підключення терміналу: " + ex2.Message;
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
				break;
			}
			goto default;
		default:
			if (Operators.CompareString(RRR.method, "withdrawal", false) == 0)
			{
				stZ = 3;
			}
			result = true;
			break;
		}
		return result;
	}

	internal int MethodEnStZ()
	{
		return RRR.method switch
		{
			"purchase" => 1, 
			"refund" => 1, 
			"cashback" => 1, 
			"withdrawal" => 1, 
			"verify" => 3, 
			"audit" => 3, 
			"identify" => 0, 
			"getmerchantlist" => 0, 
			"ping" => 0, 
			_ => 0, 
		};
	}

	private bool StartMethod()
	{
		string text = ((Operators.CompareString(RRR.type, "bpos", false) != 0) ? MethodString() : MethodStringBpos());
		All.LgT.SaveTextToLogCardserv("StartMethod", text);
		bool result;
		checked
		{
			try
			{
				switch (RRR.Connect)
				{
				case 1:
				{
					byte[] bytes2 = Encoding.ASCII.GetBytes('\0' + text + '\0');
					string text3 = "";
					int num2 = bytes2.Length - 1;
					for (int j = 0; j <= num2; j++)
					{
						text3 = text3 + bytes2[j] + " ";
					}
					All.LgT.SaveTextToLogCardserv("ARRAY", bytes2.Length.ToString(), text3);
					Port.Write(bytes2, 0, bytes2.Length);
					Thread.Sleep(333);
					if (MethodEnStZ() == 0)
					{
						TextBoxStatus.Text = StatusPort();
						All.CardservTrue = true;
						JsonToXML(TextBoxStatus.Text);
						All.LgT.SaveTextToLogCardserv("RESPONSE_" + RRR.method, TextBoxStatus.Text);
						Timer1.Stop();
						((Control)ButtonClose).Enabled = true;
						result = TextBoxStatus.Text.Length > 0;
						goto IL_04ed;
					}
					Tim = 0;
					break;
				}
				case 3:
				{
					byte[] bytes3 = Encoding.ASCII.GetBytes('\0' + text + '\0');
					string text4 = "";
					int num3 = bytes3.Length - 1;
					for (int k = 0; k <= num3; k++)
					{
						text4 = text4 + bytes3[k] + " ";
					}
					All.LgT.SaveTextToLogCardserv("ARRAY", bytes3.Length.ToString(), text4);
					Port.Write(bytes3, 0, bytes3.Length);
					Thread.Sleep(333);
					Tim = 0;
					break;
				}
				case 2:
				{
					byte[] bytes4 = Encoding.ASCII.GetBytes('\0' + text + '\0');
					string text5 = "";
					int num4 = bytes4.Length - 1;
					for (int l = 0; l <= num4; l++)
					{
						text5 = text5 + bytes4[l] + " ";
					}
					All.LgT.SaveTextToLogCardserv("ARRAY", bytes4.Length.ToString(), text5);
					Sock.Send(bytes4, bytes4.Length, SocketFlags.None);
					Thread.Sleep(333);
					if (MethodEnStZ() == 0)
					{
						if (Operators.CompareString(RRR.method, "getmerchantlist", false) == 0)
						{
							Thread.Sleep(333);
							Thread.Sleep(333);
						}
						TextBoxStatus.Text = StatusPort();
						All.CardservTrue = true;
						JsonToXML(TextBoxStatus.Text);
						All.LgT.SaveTextToLogCardserv("RESPONSE_" + RRR.method, TextBoxStatus.Text);
						Timer1.Stop();
						((Control)ButtonClose).Enabled = true;
						result = TextBoxStatus.Text.Length > 0;
						goto IL_04ed;
					}
					Tim = 0;
					break;
				}
				case 4:
				{
					byte[] bytes = Encoding.ASCII.GetBytes('\0' + text + '\0');
					string text2 = "";
					int num = bytes.Length - 1;
					for (int i = 0; i <= num; i++)
					{
						text2 = text2 + bytes[i] + " ";
					}
					All.LgT.SaveTextToLogCardserv("ARRAY", bytes.Length.ToString(), text2);
					Sock.Send(bytes, bytes.Length, SocketFlags.None);
					Thread.Sleep(333);
					Tim = 0;
					break;
				}
				case 5:
					WriteBpos(text);
					Thread.Sleep(333);
					Tim = 0;
					break;
				case 6:
					WriteBpos(text);
					Thread.Sleep(333);
					Tim = 0;
					break;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.LgT.SaveTextToLogCardserv("Port ERROR", ex2.Message);
				Timer1.Stop();
				((Control)ButtonClose).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_04ed;
			}
			Timer1.Start();
			result = true;
			goto IL_04ed;
		}
		IL_04ed:
		return result;
	}

	private void WriteBpos(string e)
	{
		checked
		{
			byte[] array = new byte[(int)Math.Round((double)e.Length / 2.0) - 1 + 1];
			int num = e.Length - 1;
			for (int i = 0; i <= num; i += 2)
			{
				array[(int)Math.Round((double)i / 2.0)] = Convert.ToByte(e.Substring(i, 2), 16);
			}
			string text = "";
			int num2 = array.Length - 1;
			for (int j = 0; j <= num2; j++)
			{
				text = text + array[j] + " ";
			}
			All.LgT.SaveTextToLogCardserv("ARRAY", array.Length.ToString(), text);
			if (RRR.Connect == 5)
			{
				Port.Write(array, 0, array.Length);
			}
			else if (RRR.Connect == 6)
			{
				ServerStream.Write(array, 0, array.Length);
			}
		}
	}

	private string MethodString()
	{
		string text = "";
		return (RRR.method switch
		{
			"purchase" => Versioned.IsNumeric((object)RRR.amountOfParts) ? ("{'method':'ServicePbP','step': 0,'params': {'amount':'" + RRR.amount + "','amountOfParts':'" + RRR.amountOfParts + "','subMerchant':'','merchantId':'" + RRR.merchantId + "'}}") : ("{'method':'Purchase','step': 0,'params': {'amount':'" + RRR.amount + "','discount':'','merchantId':'" + RRR.merchantId + "','facepay':'false','subMerchant':'" + RRR.subMerchant + "' }}"), 
			"refund" => Versioned.IsNumeric((object)RRR.amountOfParts) ? ("{'method':'ServiceRefPbP','Step': 0,'params': {'amount':'" + RRR.amount + "','agreementNum':'" + RRR.rrn + "','merchantId':'" + RRR.merchantId + "'}}") : ("{'method':'Refund','step': 0,'params': {'amount':'" + RRR.amount + "','discount':'','merchantId':'" + RRR.merchantId + "','rrn':'" + RRR.rrn + "','subMerchant':'" + RRR.subMerchant + "' }}"), 
			"cashback" => "{'method':'Cashback','step': 0,'params': {'amount':'" + RRR.amount + "','amountCash':'" + RRR.amountcash + "','merchantId':'" + RRR.merchantId + "','subMerchant':'" + RRR.subMerchant + "' }}", 
			"verify" => "{'method':'Verify','step':0,'params': {'merchantId':'" + RRR.merchantId + "'}}", 
			"audit" => "{'method':'Audit','step':0,'params': {'merchantId':'" + RRR.merchantId + "'}}", 
			"identify" => "{'method':'ServiceMessage','step':0,'params': {'msgType':'identify'}}", 
			"getmerchantlist" => "{'method':'ServiceMessage','step':0,'params': {'msgType':'getMerchantList'}}", 
			"withdrawal" => "{'method': 'Withdrawal','step': 0,'params': {'invoiceNumber':'" + RRR.invoicenumber + "'}}", 
			_ => "{'method':'PingDevice','step':0}", 
		}).Replace('\'', '"');
	}

	private string LastCheckTerminal()
	{
		string result;
		if (Operators.CompareString(RRR.method, "purchase", false) != 0)
		{
			result = "";
		}
		else
		{
			string text = "{'method':'PrintReceiptNum','step':0,'params': {'invoiceNumber':'0','prnFlag':'0'}}";
			text = text.Replace('\'', '"');
			try
			{
				byte[] bytes = Encoding.ASCII.GetBytes(text + '\0');
				switch (RRR.Connect)
				{
				case 1:
					Port.Write(bytes, 0, bytes.Length);
					goto IL_0101;
				case 3:
					Port.Write(bytes, 0, bytes.Length);
					goto IL_0101;
				case 2:
					Sock.Send(bytes, bytes.Length, SocketFlags.None);
					goto IL_0101;
				case 4:
					Sock.Send(bytes, bytes.Length, SocketFlags.None);
					goto IL_0101;
				default:
					result = "";
					break;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.LgT.SaveTextToLogCardserv("LastCheckTerminal", "Ошибка " + ex2.Message, text);
				result = "";
				ProjectData.ClearProjectError();
			}
		}
		goto IL_012a;
		IL_0101:
		Thread.Sleep(999);
		string text2 = StatusPort();
		All.LgT.SaveTextToLogCardserv("LastCheckTerminal", text2);
		result = text2;
		goto IL_012a;
		IL_012a:
		return result;
	}

	private string MethodStringBpos()
	{
		string text = "";
		HexTextBpos bP = BP;
		switch (RRR.method)
		{
		case "purchase":
			text = bP.CreateTag(1, 1) + bP.CreateTag(2, 1) + bP.CreateTag(33, RRR.merchantId) + bP.CreateTag(3, All.Bablo(RRR.amount, sDot: false));
			break;
		case "refund":
			text = bP.CreateTag(1, 1) + bP.CreateTag(2, 10) + bP.CreateTag(33, RRR.merchantId) + bP.CreateTag(3, All.Bablo(RRR.amount, sDot: false)) + bP.CreateTagASCII(18, RRR.rrn);
			break;
		case "verify":
			text = bP.CreateTag(1, 3) + bP.CreateTag(27, 1) + bP.CreateTag(33, RRR.merchantId);
			break;
		case "audit":
			text = bP.CreateTag(1, 3) + bP.CreateTag(27, 2) + bP.CreateTag(33, RRR.merchantId) + bP.CreateTag(47, 1);
			break;
		case "identify":
			text = bP.CreateTag(1, 3) + bP.CreateTag(27, 8);
			break;
		case "withdrawal":
			text = bP.CreateTag(1, 1) + bP.CreateTag(2, 11) + bP.CreateTag(33, RRR.merchantId) + bP.CreateTag(3, All.Bablo(RRR.amount, sDot: false)) + bP.CreateTag(17, RRR.invoicenumber);
			break;
		default:
			text = bP.CreateTag(1, 11);
			break;
		case "cashback":
		case "getmerchantlist":
			break;
		}
		string text2 = bP.CountLRC(text);
		text = "02" + checked((int)Math.Round((double)text.Length / 2.0)).ToString("X4") + text + text2;
		bP = null;
		return text;
	}

	private void Timer1_Tick(object sender, EventArgs e)
	{
		checked
		{
			switch (RRR.Connect)
			{
			case 1:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text2 = StatusPort().ToLower();
				if (text2.Length > 0)
				{
					Tim = 0;
					if (Operators.CompareString(JsonValue(text2, "method"), "servicemessage", false) == 0)
					{
						string jsonE = JsonValue(text2, "params");
						string text3 = JsonValue(jsonE, "laststatmsgcode");
						TextBoxStatus.Text = ServiceText(text3);
						if (Operators.CompareString(text3, "0", false) == 0)
						{
							Timer1.Stop();
							((Control)ButtonClose).Enabled = true;
							All.CardservTrue = false;
							All.LgT.SaveTextToLogCardserv("RESPONSE_0", ServiceText(text3));
						}
						else if ((Operators.CompareString(text3, "10", false) == 0) | (Operators.CompareString(text3, "3", false) == 0) | (Operators.CompareString(text3, "5", false) == 0))
						{
							stZ = 3;
						}
						else if (stZ == 2)
						{
							RequestStatusTerminal();
						}
					}
					else
					{
						Timer1.Stop();
						All.CardservTrue = true;
						JsonToXML(text2);
						TextBoxStatus.Text = text2;
						((Control)ButtonClose).Enabled = true;
						All.LgT.SaveTextToLogCardserv("RESPONSE", text2);
						((Form)this).Close();
					}
				}
				else if (stZ == 1)
				{
					stZ = 2;
					RequestStatusTerminal();
				}
				break;
			}
			case 3:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp + LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text7 = StatusPort().ToLower();
				if (text7.Length > 0)
				{
					Timer1.Stop();
					All.CardservTrue = true;
					JsonToXML(text7);
					TextBoxStatus.Text = text7;
					((Control)ButtonClose).Enabled = true;
					All.LgT.SaveTextToLogCardserv("RESPONSE", text7);
					if (MethodEnStZ() > 0)
					{
						((Form)this).Close();
					}
				}
				break;
			}
			case 2:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text4 = StatusPort().ToLower();
				if (text4.Length > 0)
				{
					Tim = 0;
					if (Operators.CompareString(JsonValue(text4, "method"), "servicemessage", false) == 0)
					{
						string jsonE2 = JsonValue(text4, "params");
						string text5 = JsonValue(jsonE2, "laststatmsgcode");
						TextBoxStatus.Text = ServiceText(text5);
						if (Operators.CompareString(text5, "0", false) == 0)
						{
							Timer1.Stop();
							((Control)ButtonClose).Enabled = true;
							All.CardservTrue = false;
							All.LgT.SaveTextToLogCardserv("RESPONSE_0", ServiceText(text5));
						}
						else if ((Operators.CompareString(text5, "10", false) == 0) | (Operators.CompareString(text5, "3", false) == 0) | (Operators.CompareString(text5, "5", false) == 0))
						{
							stZ = 3;
						}
						else if (stZ == 2)
						{
							RequestStatusTerminal();
						}
					}
					else
					{
						Timer1.Stop();
						All.CardservTrue = true;
						JsonToXML(text4);
						TextBoxStatus.Text = text4;
						((Control)ButtonClose).Enabled = true;
						All.LgT.SaveTextToLogCardserv("RESPONSE", text4);
						((Form)this).Close();
					}
				}
				else if (stZ == 1)
				{
					stZ = 2;
					RequestStatusTerminal();
				}
				break;
			}
			case 4:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp + LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text6 = StatusPort().ToLower();
				if (text6.Length > 0)
				{
					Timer1.Stop();
					All.CardservTrue = true;
					JsonToXML(text6);
					TextBoxStatus.Text = text6;
					((Control)ButtonClose).Enabled = true;
					All.LgT.SaveTextToLogCardserv("RESPONSE", text6);
					if (MethodEnStZ() > 0)
					{
						((Form)this).Close();
					}
				}
				break;
			}
			case 5:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp + LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text8 = StatusPort().ToLower();
				if (text8.Length > 0)
				{
					Timer1.Stop();
					All.CardservTrue = true;
					BposToXML(text8);
					if (Operators.CompareString(text8, "error", false) == 0)
					{
						text8 = "Помилка верифікації відповіді BPOS";
					}
					if (Operators.CompareString(RRR.method, "identify", false) == 0)
					{
						text8 = BP.get_TegX(15) + "    " + BP.get_TegX(68);
					}
					TextBoxStatus.Text = text8;
					((Control)ButtonClose).Enabled = true;
					All.LgT.SaveTextToLogCardserv("RESPONSE", text8);
					if (MethodEnStZ() > 0)
					{
						((Form)this).Close();
					}
				}
				break;
			}
			case 6:
			{
				Tim += 500;
				((Form)this).Text = "Оплата карткою                                                  " + unchecked(Tim / 1000);
				if (Tim > LimitTimeUp + LimitTimeUp)
				{
					Timer1.Stop();
					((Control)ButtonClose).Enabled = true;
					TextBoxStatus.Text = "Перевищено ліміт очікування від банківського термінала. Визначити статус останньої операцій не можливо. Натисніть кнопку ЗАКРИТИ та перевірте зв'язок з банківськім терміналом.";
					All.CardservTrue = false;
					break;
				}
				string text = StatusPort().ToLower();
				if (text.Length > 0)
				{
					Timer1.Stop();
					All.CardservTrue = true;
					BposToXML(text);
					if (Operators.CompareString(text, "error", false) == 0)
					{
						text = "Помилка верифікації відповіді BPOS";
					}
					if (Operators.CompareString(RRR.method, "identify", false) == 0)
					{
						text = BP.get_TegX(15) + "    " + BP.get_TegX(68);
					}
					TextBoxStatus.Text = text;
					((Control)ButtonClose).Enabled = true;
					All.LgT.SaveTextToLogCardserv("RESPONSE", text);
					if (MethodEnStZ() > 0)
					{
						((Form)this).Close();
					}
				}
				break;
			}
			default:
				((Control)ButtonClose).Enabled = true;
				break;
			}
		}
	}

	private string StatusPort(bool OnlyReadBPOS = false)
	{
		string result;
		checked
		{
			try
			{
				switch (RRR.Connect)
				{
				case 1:
				{
					int bytesToRead3 = Port.BytesToRead;
					if (bytesToRead3 < 1)
					{
						result = "";
					}
					else
					{
						byte[] array6 = new byte[bytesToRead3 + 1];
						Port.Read(array6, 0, bytesToRead3);
						result = Encoding.UTF8.GetString(array6, 0, array6.Length).Trim();
					}
					goto IL_0bcc;
				}
				case 3:
				{
					int bytesToRead2 = Port.BytesToRead;
					if (bytesToRead2 < 1)
					{
						result = "";
					}
					else
					{
						byte[] array4 = new byte[bytesToRead2 + 1];
						Port.Read(array4, 0, bytesToRead2);
						string @string = Encoding.UTF8.GetString(array4, 0, array4.Length);
						if (Operators.CompareString(JsonValue(@string, "method"), "servicemessage", false) == 0 && MethodEnStZ() > 0)
						{
							All.LgT.SaveTextToLogCardserv("StatusPortOldCOM", "Получено сообщение от терминала (без запроса): " + @string);
							result = "";
						}
						else
						{
							result = @string.Trim();
						}
					}
					goto IL_0bcc;
				}
				case 2:
				{
					byte[] array2 = new byte[71681];
					int count = Sock.Receive(array2, array2.Length, SocketFlags.None);
					result = Encoding.UTF8.GetString(array2, 0, count).Trim();
					goto IL_0bcc;
				}
				case 4:
				{
					byte[] array5 = new byte[71681];
					int count2 = Sock.Receive(array5, array5.Length, SocketFlags.None);
					string string2 = Encoding.UTF8.GetString(array5, 0, count2);
					if (Operators.CompareString(JsonValue(string2, "method"), "servicemessage", false) == 0 && MethodEnStZ() > 0)
					{
						All.LgT.SaveTextToLogCardserv("StatusPortOldIP", "Получено сообщение от терминала (без запроса): " + string2);
						result = "";
					}
					else
					{
						result = string2.Trim();
					}
					goto IL_0bcc;
				}
				case 5:
				{
					int bytesToRead = Port.BytesToRead;
					if (bytesToRead < 1)
					{
						result = "";
					}
					else
					{
						byte[] array3 = new byte[bytesToRead + 1];
						Port.Read(array3, 0, bytesToRead);
						string text2 = BitConverter.ToString(array3).Replace("-", string.Empty);
						All.LgT.SaveTextToLogCardserv("StatusPortBposCOM", text2);
						if (OnlyReadBPOS)
						{
							result = text2;
						}
						else if (text2.Trim().Length < 4)
						{
							result = "";
						}
						else
						{
							if (Operators.CompareString(text2.Trim().Substring(0, 4), "0600", false) == 0)
							{
								All.LgT.SaveTextToLogCardserv("StatusPortBposCOM06", text2);
								text2 = "";
								sRbpos = "";
							}
							else
							{
								if ((text2.Trim().Length > 3) & (text2.Trim().Length < 9))
								{
									if (Operators.CompareString(text2.Trim(), "0612", false) == 0)
									{
										sRbpos = "0612";
										text2 = "";
									}
									else if (Operators.CompareString(text2.Trim(), "6200", false) == 0)
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposCOMPlus6200", text2);
										sRbpos = "";
										text2 = "";
									}
									else
									{
										sRbpos = text2.Trim();
										sRbpos = sRbpos.Substring(0, sRbpos.Length - 2);
										text2 = "";
									}
								}
								else if (Operators.CompareString(sRbpos, "", false) != 0 && text2.Trim().Length > 8)
								{
									text2 = sRbpos + text2.Trim();
									All.LgT.SaveTextToLogCardserv("StatusPortBposCOMPlus", sRbpos, text2);
									sRbpos = "";
								}
								if (BposTS.Length == 0 && !BP.Parsing(text2))
								{
									text2 = "02" + text2;
									if (!BP.Parsing(text2))
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposCOMError", BposTS);
										BposTS = "";
										text2 = "error";
										result = text2;
										goto IL_0bcc;
									}
								}
								if (BP.get_TegX(31).Length > 0)
								{
									TextBoxStatus.Text = BP.get_TegX(31);
									text2 = "";
									WriteBpos("06");
								}
								else if (Operators.CompareString(BposTS, "", false) == 0)
								{
									if (Operators.CompareString(text2.Trim().Substring(0, 4), "0604", false) == 0)
									{
										text2 = "";
										result = text2;
										goto IL_0bcc;
									}
									BposTS = text2;
									text2 = "";
									WriteBpos("06");
									if (BP.Parsing(BposTS))
									{
										if ((Operators.CompareString(RRR.method, "verify", false) == 0) | (Operators.CompareString(RRR.method, "identify", false) == 0) | (Operators.CompareString(RRR.method, "audit", false) == 0) | (Operators.CompareString(RRR.method, "withdrawal", false) == 0))
										{
											text2 = BposTS;
											BposTS = "";
										}
										else if (Versioned.IsNumeric((object)BP.get_TegX(14)))
										{
											if (Conversions.ToInteger(BP.get_TegX(14).Trim()) == 0)
											{
												WriteBpos("02000c00000001000000040000000500");
											}
											else
											{
												text2 = BposTS;
												BposTS = "";
											}
										}
										else
										{
											text2 = BposTS;
											BposTS = "";
										}
									}
									else
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposCOMError", BposTS);
										BposTS = "";
										text2 = "error";
									}
								}
								else
								{
									text2 = BposTS;
									BposTS = "";
									WriteBpos("06");
								}
							}
							if (Operators.CompareString(text2, "", false) != 0)
							{
								All.LgT.SaveTextToLogCardserv("StatusPortBposCOM", text2);
							}
							result = text2.Trim();
						}
					}
					goto IL_0bcc;
				}
				case 6:
				{
					int available = ClientSocket.Available;
					if (available < 1)
					{
						result = "";
					}
					else
					{
						byte[] array = new byte[available * 2 - 1 + 1];
						ServerStream.Read(array, 0, array.Length);
						string text = BitConverter.ToString(array).Replace("-", string.Empty);
						text = text.Substring(0, available * 2);
						All.LgT.SaveTextToLogCardserv("StatusPortBposIP", text);
						if (OnlyReadBPOS)
						{
							result = text;
						}
						else if (text.Trim().Length < 4)
						{
							result = "";
						}
						else
						{
							if (Operators.CompareString(text.Trim().Substring(0, 4), "0600", false) == 0)
							{
								All.LgT.SaveTextToLogCardserv("StatusPortBposIP06", text);
								text = "";
								sRbpos = "";
							}
							else
							{
								if ((text.Trim().Length > 3) & (text.Trim().Length < 9))
								{
									if (Operators.CompareString(text.Trim(), "0612", false) == 0)
									{
										sRbpos = "0612";
										text = "";
									}
									else if (Operators.CompareString(text.Trim(), "6200", false) == 0)
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposIPPlus6200", text);
										sRbpos = "";
										text = "";
									}
									else
									{
										sRbpos = text.Trim();
										sRbpos = sRbpos.Substring(0, sRbpos.Length - 2);
										text = "";
									}
								}
								else if (Operators.CompareString(sRbpos, "", false) != 0 && text.Trim().Length > 8)
								{
									if ((Operators.CompareString(sRbpos.Trim(), "06", false) == 0) & (Operators.CompareString(text.Substring(0, 2), "02", false) != 0))
									{
										sRbpos = sRbpos.Trim() + "02";
									}
									text = sRbpos + text.Trim();
									All.LgT.SaveTextToLogCardserv("StatusPortBposBposIPPlus", sRbpos, text);
									sRbpos = "";
								}
								if (BposTS.Length == 0 && !BP.Parsing(text))
								{
									text = "02" + text;
									if (!BP.Parsing(text))
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposIPError+02", text);
										BposTS = "";
										text = "error";
										result = text;
										goto IL_0bcc;
									}
								}
								if (BP.get_TegX(31).Length > 0)
								{
									TextBoxStatus.Text = BP.get_TegX(31);
									text = "";
									WriteBpos("06");
								}
								else if (Operators.CompareString(BposTS, "", false) == 0)
								{
									if (Operators.CompareString(text.Trim().Substring(0, 4), "0604", false) == 0)
									{
										text = "";
										result = text;
										goto IL_0bcc;
									}
									BposTS = text;
									text = "";
									WriteBpos("06");
									if (BP.Parsing(BposTS))
									{
										if ((Operators.CompareString(RRR.method, "verify", false) == 0) | (Operators.CompareString(RRR.method, "identify", false) == 0) | (Operators.CompareString(RRR.method, "audit", false) == 0) | (Operators.CompareString(RRR.method, "withdrawal", false) == 0))
										{
											text = BposTS;
											BposTS = "";
										}
										else if (Versioned.IsNumeric((object)BP.get_TegX(14)))
										{
											if (Conversions.ToInteger(BP.get_TegX(14).Trim()) == 0)
											{
												WriteBpos("02000c00000001000000040000000500");
											}
											else
											{
												text = BposTS;
												BposTS = "";
											}
										}
										else
										{
											text = BposTS;
											BposTS = "";
										}
									}
									else
									{
										All.LgT.SaveTextToLogCardserv("StatusPortBposIPError", BposTS);
										BposTS = "";
										text = "error";
									}
								}
								else
								{
									text = BposTS;
									BposTS = "";
									WriteBpos("06");
								}
							}
							if (Operators.CompareString(text, "", false) != 0)
							{
								All.LgT.SaveTextToLogCardserv("StatusPortBposIP", text);
							}
							result = text.Trim();
						}
					}
					goto IL_0bcc;
				}
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				if ((RRR.Connect == 1) | (RRR.Connect == 3) | (RRR.Connect == 5) | (RRR.Connect == 6))
				{
					All.LgT.SaveTextToLogCardserv("StatusPort", ex2.Message);
				}
				result = "";
				ProjectData.ClearProjectError();
				goto IL_0bcc;
			}
			All.LgT.SaveTextToLogCardserv("StatusPortERROR", "Помилка читання даних");
			result = "";
			goto IL_0bcc;
		}
		IL_0bcc:
		return result;
	}

	private void ButtonClose_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void RequestStatusTerminal()
	{
		string text = "{'method':'ServiceMessage','step':0,'params':{'msgType':'getLastStatMsgCode'}}";
		text = text.Replace('\'', '"');
		try
		{
			byte[] bytes = Encoding.ASCII.GetBytes(text + '\0');
			switch (RRR.Connect)
			{
			case 1:
				Port.Write(bytes, 0, bytes.Length);
				break;
			case 3:
				Port.Write(bytes, 0, bytes.Length);
				break;
			case 2:
				Sock.Send(bytes, bytes.Length, SocketFlags.None);
				break;
			case 4:
				Sock.Send(bytes, bytes.Length, SocketFlags.None);
				break;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.LgT.SaveTextToLogCardserv("RequestStatusTerminal", "Ошибка " + ex2.Message, text);
			ProjectData.ClearProjectError();
		}
	}

	private string ServiceText(string e)
	{
		return e switch
		{
			"0" => "Зарезервовано (status code is not available)", 
			"1" => "Картка була зчитана (card was read)", 
			"2" => "Використана картка з чипом (used a chip card)", 
			"3" => "Триває авторизація (authorization in progress)", 
			"4" => "Очікуеться дія касира (waiting for cashier action)", 
			"5" => "Виконується друк чека (printing receipt)", 
			"6" => "Введіть ПИН код (pin entry is needed)", 
			"7" => "Картка прибрана  (card was removed)", 
			"8" => "EMV multi aid’s", 
			"9" => "Очікується карта (waiting for card)", 
			"10" => "Операція виконується (in progress)", 
			"11" => "Вдала операція (correct transaction)", 
			_ => "", 
		};
	}

	private string TextToJSON(string t)
	{
		string text = t.Trim();
		string oldValue = "\n";
		string text2 = text.Replace(oldValue, "");
		oldValue = "\r";
		string text3 = text2.Replace(oldValue, "");
		oldValue = ",\"";
		string text4 = text3.Replace(oldValue, "HGB99HGB01");
		oldValue = ", \"";
		string text5 = text4.Replace(oldValue, "HGB99HGB02");
		oldValue = ":\"";
		string text6 = text5.Replace(oldValue, "HGB99HGB03");
		oldValue = ": \"";
		string text7 = text6.Replace(oldValue, "HGB99HGB04");
		oldValue = "{\"";
		string text8 = text7.Replace(oldValue, "HGB99HGB05");
		oldValue = "{ \"";
		string text9 = text8.Replace(oldValue, "HGB99HGB06");
		oldValue = "\",";
		string text10 = text9.Replace(oldValue, "HGB07");
		oldValue = "\" ,";
		string text11 = text10.Replace(oldValue, "HGB08");
		oldValue = "\":";
		string text12 = text11.Replace(oldValue, "HGB09");
		oldValue = "\" :";
		string text13 = text12.Replace(oldValue, "HGB10");
		oldValue = "\"}";
		string text14 = text13.Replace(oldValue, "HGB11");
		oldValue = "\" }";
		string text15 = text14.Replace(oldValue, "HGB12");
		oldValue = "\"HGB99";
		string text16 = text15.Replace(oldValue, "HGB13");
		oldValue = "\" HGB99";
		string text17 = text16.Replace(oldValue, "HGB14").Replace('"', '\'');
		oldValue = ",\"";
		string text18 = text17.Replace("HGB01", oldValue);
		oldValue = ", \"";
		string text19 = text18.Replace("HGB02", oldValue);
		oldValue = ":\"";
		string text20 = text19.Replace("HGB03", oldValue);
		oldValue = ": \"";
		string text21 = text20.Replace("HGB04", oldValue);
		oldValue = "{\"";
		string text22 = text21.Replace("HGB05", oldValue);
		oldValue = "{ \"";
		string text23 = text22.Replace("HGB06", oldValue);
		oldValue = "\",";
		string text24 = text23.Replace("HGB07", oldValue);
		oldValue = "\" ,";
		string text25 = text24.Replace("HGB08", oldValue);
		oldValue = "\":";
		string text26 = text25.Replace("HGB09", oldValue);
		oldValue = "\" :";
		string text27 = text26.Replace("HGB10", oldValue);
		oldValue = "\"}";
		string text28 = text27.Replace("HGB11", oldValue);
		oldValue = "\" }";
		string text29 = text28.Replace("HGB12", oldValue);
		oldValue = "\"";
		string text30 = text29.Replace("HGB13", oldValue);
		oldValue = "\" ";
		return text30.Replace("HGB14", oldValue).Replace("HGB99", "");
	}

	private void JsonToXML(string eJson)
	{
		Thread.Sleep(270);
		string text = StatusPort();
		if (text.Length > 0)
		{
			eJson += text;
		}
		eJson = eJson.ToLower();
		if (Operators.CompareString(JsonValue(eJson, "error"), "", false) == 0)
		{
			eJson = TextToJSON(eJson);
		}
		string text2 = JsonValue(eJson, "error");
		string text3 = JsonValue(eJson, "errordescription");
		if ((Operators.CompareString(text2, "true", false) == 0) | (Operators.CompareString(text2, "", false) == 0))
		{
			if (Operators.CompareString(text2, "", false) == 0)
			{
				text2 = "true";
				if (Operators.CompareString(text3, "", false) == 0)
				{
					text3 = "відповіді від терміналу немає";
				}
			}
			All.B.CurrentStatus = "error=" + text2 + "_errordescription=" + text3;
			All.CardservTrue = false;
			return;
		}
		All.CardservTrue = true;
		string jsonE = JsonValue(eJson, "params");
		if (MethodEnStZ() != 1)
		{
			All.B.CurrentStatus = "error=" + text2 + "_errordescription=" + text3;
			string text4 = JsonValue(jsonE, "vendor");
			string text5 = JsonValue(jsonE, "model");
			if (text5.Length > 0)
			{
				All.B.CurrentStatus = All.B.CurrentStatus + "_model=" + text5;
			}
			if (text4.Length > 0)
			{
				All.B.CurrentStatus = All.B.CurrentStatus + "_vendor=" + text4;
			}
			All.CardservTrue = true;
			return;
		}
		string text6 = JsonValue(jsonE, "amount");
		string text7 = JsonValue(jsonE, "paymentsystem");
		string text8 = JsonValue(jsonE, "bankacquirer");
		string text9 = JsonValue(jsonE, "approvalcode");
		string text10 = JsonValue(jsonE, "invoicenumber");
		string text11 = JsonValue(jsonE, "merchant");
		string text12 = JsonValue(jsonE, "pan");
		string text13 = JsonValue(jsonE, "rrn");
		string text14 = JsonValue(jsonE, "terminalid");
		if (Operators.CompareString(RRR.method, "purchase", false) == 0 && !ComparisonAmounts(RRR.amount, text6))
		{
			All.B.CurrentStatus += "_error=9999_errordescription=Помилка верифікації суми JSON";
			All.CardservTrue = false;
			return;
		}
		All.B.CurrentStatus = "amount=" + text6 + "_paymentsystem=" + text7 + "_bankacquirer=" + text8 + "_approvalcode=" + text9 + "_invoicenumber=" + text10 + "_merchant=" + text11 + "_pan=" + text12 + "_rrn=" + text13 + "_terminalid=" + text14;
		All.B.CurrentStatus = All.B.CurrentStatus + "_error=" + text2 + "_errordescription=" + text3 + "_merchantId=" + RRR.merchantId;
		All.CardservTrue = true;
	}

	private void BposToXML(string eBpos)
	{
		if (Operators.CompareString(eBpos, "error", false) == 0)
		{
			All.B.CurrentStatus += "_error=9999_errordescription=Помилка верифікації відповіді BPOS";
			All.CardservTrue = false;
			return;
		}
		if (Operators.CompareString(RRR.method, "purchase", false) == 0 && !ComparisonAmounts(RRR.amount, BP.get_TegX(3)))
		{
			All.B.CurrentStatus += "_error=9999_errordescription=Помилка верифікації суми BPOS";
			All.CardservTrue = false;
			return;
		}
		string text = "true";
		if (Operators.CompareString(BP.get_TegX(14), "0", false) == 0)
		{
			text = "false";
		}
		All.B.CurrentStatus = All.B.CurrentStatus + "_error=" + text + "_errornumber=" + BP.get_TegX(14) + "_errordescription= " + BP.get_TegX(15) + "_method=" + BP.get_TegX(2);
		All.B.CurrentStatus = All.B.CurrentStatus + "_amount=" + BP.get_TegX(3) + "_paymentsystem=" + BP.get_TegX(26) + "_bankacquirer=" + BP.get_TegX(71) + "_approvalcode=" + BP.get_TegX(16) + "_invoicenumber=" + BP.get_TegX(17) + "_merchant=" + BP.get_TegX(6) + "_pan=" + BP.get_TegX(4) + "_rrn=" + BP.get_TegX(18) + "_terminalid=" + BP.get_TegX(5) + "_merchantId=" + RRR.merchantId;
		if (Operators.CompareString(RRR.method, "identify", false) == 0)
		{
			All.B.CurrentStatus += "_model=-_vendor=bpos";
		}
		if (Versioned.IsNumeric((object)BP.get_TegX(14)))
		{
			if (Conversions.ToInteger(BP.get_TegX(14)) == 0)
			{
				All.CardservTrue = true;
			}
			else
			{
				All.CardservTrue = false;
			}
		}
		else
		{
			All.CardservTrue = false;
		}
	}

	private bool ComparisonAmounts(string s1, string s2)
	{
		if (Operators.CompareString(RRR.dest.ToLower(), "demo", false) == 0)
		{
			return true;
		}
		s1 = s1.Trim();
		s2 = s2.Trim();
		s2 = s2.Replace(".", "");
		s2 = s2.Replace(",", "");
		if (s2.Length == 1)
		{
			s2 = "0" + s2;
		}
		if (s2.Length == 2)
		{
			s2 = "0" + s2;
		}
		s1 = All.Bablo(s1, sDot: false);
		if (Operators.CompareString(s1, s2, false) != 0)
		{
			All.LgT.SaveTextToLogCardserv("Сума запиту не відповідає сумі відповіді ", s1 + "<>" + s2);
			return false;
		}
		return true;
	}

	private string JsonValue(string JsonE, string KeyE)
	{
		string result;
		try
		{
			result = JObject.Parse(JsonE)[KeyE].ToString().Trim().ToLower();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private void Demo()
	{
		string text = "";
		switch (RRR.method)
		{
		case "purchase":
		{
			text = "{'method':'purchase','step':0,'params':{'trnstatus':'1','adv':'приватбанк','adv2p':' ','bankacquirer':'приватбанк','paymentsystem':'visa','rrnext':'316614027925','textmess':'','amount':'0.60','approvalcode':'038314','capturereference':'','cardexpirydate':'3112','date':'15.06.2023','discount':'0.00','hstfld63sf89':'','invoicenumber':'demo994300','issuername':'visa credit','merchant':'demo0746','pan':'4486********2575','posconditioncode':'00','posentrymode':'071','processingcode':'000000','receipt':' ','responsecode':'0000','rrn':'demorrn12345','terminalid':'demo0746','time':'17:13:56','track1':'','signverif':'0','txntype':'1'},'error':false,'errordescription':''}";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом вікно в якому ви бачите це повідомлення закриється автоматично при успішній транзакції.";
			break;
		}
		case "refund":
		{
			text = "{'method':'refund','step':0,'params':{'trnstatus':'1','adv':'приватбанк','adv2p':'беремо i робимо!','bankacquirer':'приватбанк','paymentsystem':'visa','rrnext':'','textmess':'','amount':'0.60','approvalcode':'demo12345','capturereference':'','cardexpirydate':'3112','cardholdername':'','date':'15.06.2023','discount':'0.00','hstfld63sf89':'','invoicenumber':'1042075581','issuername':'visa credit','merchant':'demo0746','pan':'4486********2575','posconditioncode':'00','posentrymode':'071','processingcode':'000000','receipt':' ','responsecode':'0000','rrn':'demo','terminalid':'demo0746','time':'17:46:19','track1':'','txntype':'2'},'error':false,'errordescription':''}";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом вікно в якому ви бачите це повідомлення закриється автоматично при успішній транзакції.";
			break;
		}
		case "cashback":
		{
			text = "{'method':'cashback','step':0,'params':{'result':'oперація успішна','resultcashback':'true','adv':'приватбанк','adv2p':'беремо i робимо!','bankacquirer':'приватбанк','paymentsystem':'visa','rrnext':'316614191368','submerchant':'i12345678910','textmess':'','amount':'1.60','amountcash':'1.00','approvalcode':'014315','capturereference':'','cardexpirydate':'3112','date':'15.06.2023','discount':'0.00','hstfld63sf89':'','invoicenumber':'demo091713','issuername':'visa credit','merchant':'demo0746','pan':'4486********2575','posconditioncode':'00','posentrymode':'071','processingcode':'090000','receipt':'','responsecode':'0000','rrn':'demo37276622','terminalid':'demo0746','time':'17:52:36','track1':''},'error':false,'errordescription':''}";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом вікно в якому ви бачите це повідомлення закриється автоматично при успішній транзакції.";
			break;
		}
		case "verify":
		{
			text = "";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом відбувається друк звіту на друкованому пристрої терміналу. У програму повертається лише статус виконання операції на терміналі (истина/ложь)";
			break;
		}
		case "audit":
		{
			text = "";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом відбувається друк звіту на друкованому пристрої терміналу. У програму повертається лише статус виконання операції на терміналі (истина/ложь)";
			break;
		}
		case "identify":
			text = "{'method':'ServiceMessage','step':0,'params':{'msgType':'identify','result':'true','vendor':'webcheck','model':'demo'},'error':false,'errorDescription':''}";
			break;
		case "withdrawal":
		{
			text = "{'method':'withdrawal','step':0,'params':{'trnstatus':'1','adv':'приватбанк','adv2p':'беремо i робимо!','bankacquirer':'приватбанк','paymentsystem':'visa','rrnext':'','textmess':'','amount':'0.60','approvalcode':'demo12345','capturereference':'','cardexpirydate':'3112','cardholdername':'','date':'15.06.2023','discount':'0.00','hstfld63sf89':'','invoicenumber':'1042075581','issuername':'visa credit','merchant':'demo0746','pan':'4486********2575','posconditioncode':'00','posentrymode':'071','processingcode':'000000','receipt':' ','responsecode':'0000','rrn':'demo','terminalid':'demo0746','time':'17:46:19','track1':'','txntype':'2'},'error':false,'errordescription':''}";
			TextBox textBoxStatus;
			(textBoxStatus = TextBoxStatus).Text = textBoxStatus.Text + "     При роботі з підключеним банківським терміналом вікно в якому ви бачите це повідомлення закриється автоматично при успішній транзакції.";
			break;
		}
		default:
			text = "";
			break;
		}
		if (text.Length > 0)
		{
			text = text.Replace('\'', '"');
			JsonToXML(text);
		}
	}

	private void PingT()
	{
		string text = "{'method':'PingDevice','Step':0}";
		byte[] bytes = Encoding.ASCII.GetBytes('\0' + text + '\0');
		Port.Write(bytes, 0, bytes.Length);
		Thread.Sleep(333);
		TextBoxStatus.Text = StatusPort();
	}

	private string htmlRRN(string e)
	{
		e = e.ToLower();
		checked
		{
			try
			{
				int num = Strings.InStr(e, "rrn: ", (CompareMethod)0);
				string text = e.Substring(num + 4, 12);
				if (Versioned.IsNumeric((object)text))
				{
					return text;
				}
				num = Strings.InStr(e, "rrn:", (CompareMethod)0);
				text = e.Substring(num + 3, 12);
				if (Versioned.IsNumeric((object)text))
				{
					return text;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
			return "";
		}
	}

	private string htmlCN(string e)
	{
		e = e.ToLower();
		int num = 6;
		int length = 1;
		checked
		{
			try
			{
				int num2 = Strings.InStr(e.ToLower(), "чек n: ", (CompareMethod)0);
				if (num2 == 0)
				{
					num2 = Strings.InStr(e.ToLower(), "чек № ", (CompareMethod)0);
					num = 5;
					if (num2 == 0)
					{
						num2 = Strings.InStr(e.ToLower(), "чек №", (CompareMethod)0);
						num = 4;
						if (num2 == 0)
						{
							num2 = Strings.InStr(e.ToLower(), "чек n:", (CompareMethod)0);
							num = 5;
							if (num2 == 0)
							{
								return "";
							}
						}
					}
				}
				int num3 = 1;
				do
				{
					if (!Versioned.IsNumeric((object)e.Substring(num2 + num, num3)))
					{
						length = num3 - 1;
						break;
					}
					num3++;
				}
				while (num3 <= 12);
				string text = e.Substring(num2 + num, length);
				if (Versioned.IsNumeric((object)text))
				{
					return text;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
			return "";
		}
	}

	private void Timer2_Tick(object sender, EventArgs e)
	{
		((Control)ButtonClose).Enabled = true;
		Timer2.Stop();
	}
}
