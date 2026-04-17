using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormMail : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("OkSave")]
	private Button _OkSave;

	[CompilerGenerated]
	[AccessedThroughProperty("SendTest")]
	private Button _SendTest;

	[CompilerGenerated]
	[AccessedThroughProperty("eSendEmail")]
	private CheckBox _eSendEmail;

	private SendingMail SMt;

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ePass")]
	internal virtual TextBox ePass
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("eMa")]
	internal virtual TextBox eMa
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button OkSave
	{
		[CompilerGenerated]
		get
		{
			return _OkSave;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkSave_Click;
			Button okSave = _OkSave;
			if (okSave != null)
			{
				((Control)okSave).Click -= eventHandler;
			}
			_OkSave = value;
			okSave = _OkSave;
			if (okSave != null)
			{
				((Control)okSave).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox2")]
	internal virtual GroupBox GroupBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("SendMailToAdd")]
	internal virtual TextBox SendMailToAdd
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SendTest
	{
		[CompilerGenerated]
		get
		{
			return _SendTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SendTest_Click;
			Button sendTest = _SendTest;
			if (sendTest != null)
			{
				((Control)sendTest).Click -= eventHandler;
			}
			_SendTest = value;
			sendTest = _SendTest;
			if (sendTest != null)
			{
				((Control)sendTest).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("eT")]
	internal virtual TextBox eT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ePort")]
	internal virtual TextBox ePort
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("eHost")]
	internal virtual TextBox eHost
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("eUDC")]
	internal virtual CheckBox eUDC
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("eSSL")]
	internal virtual CheckBox eSSL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox eSendEmail
	{
		[CompilerGenerated]
		get
		{
			return _eSendEmail;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = eSendEmail_CheckedChanged;
			CheckBox val = _eSendEmail;
			if (val != null)
			{
				val.CheckedChanged -= eventHandler;
			}
			_eSendEmail = value;
			val = _eSendEmail;
			if (val != null)
			{
				val.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
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

	[field: AccessedThroughProperty("Label6")]
	internal virtual Label Label6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormMail()
	{
		((Form)this).Load += FormMail_Load;
		SMt = new SendingMail();
		InitializeComponent();
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
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_00b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Expected O, but got Unknown
		//IL_00c1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cb: Expected O, but got Unknown
		//IL_00cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d6: Expected O, but got Unknown
		//IL_00d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e1: Expected O, but got Unknown
		//IL_0229: Unknown result type (might be due to invalid IL or missing references)
		//IL_0233: Expected O, but got Unknown
		//IL_0251: Unknown result type (might be due to invalid IL or missing references)
		//IL_0275: Unknown result type (might be due to invalid IL or missing references)
		//IL_02e4: Unknown result type (might be due to invalid IL or missing references)
		//IL_02ee: Expected O, but got Unknown
		//IL_0369: Unknown result type (might be due to invalid IL or missing references)
		//IL_0373: Expected O, but got Unknown
		//IL_03ee: Unknown result type (might be due to invalid IL or missing references)
		//IL_03f8: Expected O, but got Unknown
		//IL_0473: Unknown result type (might be due to invalid IL or missing references)
		//IL_047d: Expected O, but got Unknown
		//IL_04f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_04fe: Expected O, but got Unknown
		//IL_0817: Unknown result type (might be due to invalid IL or missing references)
		//IL_0821: Expected O, but got Unknown
		//IL_0842: Unknown result type (might be due to invalid IL or missing references)
		//IL_0939: Unknown result type (might be due to invalid IL or missing references)
		//IL_0943: Expected O, but got Unknown
		//IL_0964: Unknown result type (might be due to invalid IL or missing references)
		//IL_0988: Unknown result type (might be due to invalid IL or missing references)
		//IL_09f7: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a01: Expected O, but got Unknown
		//IL_0aed: Unknown result type (might be due to invalid IL or missing references)
		//IL_0af7: Expected O, but got Unknown
		//IL_0b18: Unknown result type (might be due to invalid IL or missing references)
		//IL_0c2a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0c34: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormMail));
		GroupBox1 = new GroupBox();
		Label5 = new Label();
		Label4 = new Label();
		Label3 = new Label();
		Label1 = new Label();
		Label2 = new Label();
		eUDC = new CheckBox();
		eSSL = new CheckBox();
		eT = new TextBox();
		ePort = new TextBox();
		eHost = new TextBox();
		ePass = new TextBox();
		eMa = new TextBox();
		OkSave = new Button();
		GroupBox2 = new GroupBox();
		Label6 = new Label();
		eSendEmail = new CheckBox();
		SendTest = new Button();
		SendMailToAdd = new TextBox();
		((Control)GroupBox1).SuspendLayout();
		((Control)GroupBox2).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)GroupBox1).Anchor = (AnchorStyles)9;
		((Control)GroupBox1).Controls.Add((Control)(object)Label5);
		((Control)GroupBox1).Controls.Add((Control)(object)Label4);
		((Control)GroupBox1).Controls.Add((Control)(object)Label3);
		((Control)GroupBox1).Controls.Add((Control)(object)Label1);
		((Control)GroupBox1).Controls.Add((Control)(object)Label2);
		((Control)GroupBox1).Controls.Add((Control)(object)eUDC);
		((Control)GroupBox1).Controls.Add((Control)(object)eSSL);
		((Control)GroupBox1).Controls.Add((Control)(object)eT);
		((Control)GroupBox1).Controls.Add((Control)(object)ePort);
		((Control)GroupBox1).Controls.Add((Control)(object)eHost);
		((Control)GroupBox1).Controls.Add((Control)(object)ePass);
		((Control)GroupBox1).Controls.Add((Control)(object)eMa);
		((Control)GroupBox1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox1).Location = new Point(12, 11);
		((Control)GroupBox1).Margin = new Padding(3, 2, 3, 2);
		((Control)GroupBox1).Name = "GroupBox1";
		((Control)GroupBox1).Padding = new Padding(3, 2, 3, 2);
		((Control)GroupBox1).Size = new Size(675, 272);
		((Control)GroupBox1).TabIndex = 6;
		GroupBox1.TabStop = false;
		GroupBox1.Text = "Налаштування";
		Label5.AutoSize = true;
		((Control)Label5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label5).Location = new Point(17, 214);
		((Control)Label5).Name = "Label5";
		((Control)Label5).Size = new Size(85, 25);
		((Control)Label5).TabIndex = 11;
		Label5.Text = "Таймінг";
		Label4.AutoSize = true;
		((Control)Label4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label4).Location = new Point(17, 165);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(60, 25);
		((Control)Label4).TabIndex = 10;
		Label4.Text = "Порт";
		Label3.AutoSize = true;
		((Control)Label3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label3).Location = new Point(17, 118);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(141, 25);
		((Control)Label3).TabIndex = 9;
		Label3.Text = "Сервер SMTP";
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(17, 74);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(80, 25);
		((Control)Label1).TabIndex = 8;
		Label1.Text = "Пароль";
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(17, 33);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(193, 25);
		((Control)Label2).TabIndex = 7;
		Label2.Text = "Електронна адреса";
		((ButtonBase)eUDC).AutoSize = true;
		((Control)eUDC).Location = new Point(430, 210);
		((Control)eUDC).Name = "eUDC";
		((Control)eUDC).Size = new Size(239, 29);
		((Control)eUDC).TabIndex = 6;
		((ButtonBase)eUDC).Text = "Use Default Credentials";
		((ButtonBase)eUDC).UseVisualStyleBackColor = true;
		((ButtonBase)eSSL).AutoSize = true;
		((Control)eSSL).Location = new Point(430, 161);
		((Control)eSSL).Name = "eSSL";
		((Control)eSSL).Size = new Size(239, 29);
		((Control)eSSL).TabIndex = 5;
		((ButtonBase)eSSL).Text = "Використовувати SSL";
		((ButtonBase)eSSL).UseVisualStyleBackColor = true;
		((Control)eT).Location = new Point(230, 209);
		((Control)eT).Name = "eT";
		((Control)eT).Size = new Size(170, 30);
		((Control)eT).TabIndex = 4;
		eT.TextAlign = (HorizontalAlignment)2;
		((Control)ePort).Location = new Point(230, 160);
		((Control)ePort).Name = "ePort";
		((Control)ePort).Size = new Size(170, 30);
		((Control)ePort).TabIndex = 3;
		ePort.TextAlign = (HorizontalAlignment)2;
		((Control)eHost).Location = new Point(230, 113);
		((Control)eHost).Name = "eHost";
		((Control)eHost).Size = new Size(342, 30);
		((Control)eHost).TabIndex = 2;
		eHost.TextAlign = (HorizontalAlignment)2;
		((Control)ePass).Location = new Point(230, 69);
		((Control)ePass).Name = "ePass";
		((Control)ePass).Size = new Size(342, 30);
		((Control)ePass).TabIndex = 1;
		ePass.TextAlign = (HorizontalAlignment)2;
		((Control)eMa).Location = new Point(230, 28);
		((Control)eMa).Name = "eMa";
		((Control)eMa).Size = new Size(342, 30);
		((Control)eMa).TabIndex = 0;
		eMa.TextAlign = (HorizontalAlignment)2;
		((Control)OkSave).Anchor = (AnchorStyles)9;
		((Control)OkSave).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkSave).Location = new Point(496, 38);
		((Control)OkSave).Margin = new Padding(3, 2, 3, 2);
		((Control)OkSave).Name = "OkSave";
		((Control)OkSave).Size = new Size(164, 39);
		((Control)OkSave).TabIndex = 5;
		((ButtonBase)OkSave).Text = "Зберегти";
		((ButtonBase)OkSave).UseVisualStyleBackColor = true;
		((Control)OkSave).Visible = false;
		((Control)GroupBox2).Anchor = (AnchorStyles)9;
		((Control)GroupBox2).Controls.Add((Control)(object)Label6);
		((Control)GroupBox2).Controls.Add((Control)(object)eSendEmail);
		((Control)GroupBox2).Controls.Add((Control)(object)SendTest);
		((Control)GroupBox2).Controls.Add((Control)(object)OkSave);
		((Control)GroupBox2).Controls.Add((Control)(object)SendMailToAdd);
		((Control)GroupBox2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox2).Location = new Point(12, 287);
		((Control)GroupBox2).Margin = new Padding(3, 2, 3, 2);
		((Control)GroupBox2).Name = "GroupBox2";
		((Control)GroupBox2).Padding = new Padding(3, 2, 3, 2);
		((Control)GroupBox2).Size = new Size(675, 186);
		((Control)GroupBox2).TabIndex = 7;
		GroupBox2.TabStop = false;
		GroupBox2.Text = "Надсилати повідомлення";
		Label6.AutoSize = true;
		((Control)Label6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label6).Location = new Point(17, 45);
		((Control)Label6).Name = "Label6";
		((Control)Label6).Size = new Size(374, 25);
		((Control)Label6).TabIndex = 10;
		Label6.Text = "Вкажіть електронну адресу для тесту:";
		((ButtonBase)eSendEmail).AutoSize = true;
		((Control)eSendEmail).Location = new Point(22, 118);
		((Control)eSendEmail).Name = "eSendEmail";
		((Control)eSendEmail).Size = new Size(295, 54);
		((Control)eSendEmail).TabIndex = 9;
		((ButtonBase)eSendEmail).Text = "відправляти повідомлення, \r\nякщо зазначений eMail";
		((ButtonBase)eSendEmail).UseVisualStyleBackColor = true;
		((Control)SendTest).Anchor = (AnchorStyles)9;
		((Control)SendTest).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SendTest).Location = new Point(496, 103);
		((Control)SendTest).Margin = new Padding(3, 2, 3, 2);
		((Control)SendTest).Name = "SendTest";
		((Control)SendTest).Size = new Size(164, 69);
		((Control)SendTest).TabIndex = 8;
		((ButtonBase)SendTest).Text = "Зберегти та відправити ";
		((ButtonBase)SendTest).UseVisualStyleBackColor = true;
		((Control)SendMailToAdd).Location = new Point(22, 73);
		((Control)SendMailToAdd).Name = "SendMailToAdd";
		((Control)SendMailToAdd).Size = new Size(369, 30);
		((Control)SendMailToAdd).TabIndex = 1;
		SendMailToAdd.TextAlign = (HorizontalAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(699, 484);
		((Control)this).Controls.Add((Control)(object)GroupBox2);
		((Control)this).Controls.Add((Control)(object)GroupBox1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormMail";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Налаштування SMTP";
		((Control)GroupBox1).ResumeLayout(false);
		((Control)GroupBox1).PerformLayout();
		((Control)GroupBox2).ResumeLayout(false);
		((Control)GroupBox2).PerformLayout();
		((Control)this).ResumeLayout(false);
	}

	private void FormMail_Load(object sender, EventArgs e)
	{
		TypMail typMail = SMt.LoadEmail();
		eMa.Text = typMail.From;
		ePass.Text = typMail.Pass;
		eHost.Text = typMail.Host;
		eT.Text = typMail.TimeOut.ToString();
		ePort.Text = typMail.Port.ToString();
		eSSL.Checked = typMail.EnableSSL;
		eUDC.Checked = typMail.UseDefaultCredentials;
		eSendEmail.Checked = typMail.SendToEmail;
		if (!typMail.SendToEmail)
		{
			((Control)eSendEmail).Enabled = false;
		}
	}

	private bool SendTestMail()
	{
		SendingMail sendingMail = new SendingMail();
		string toMail = SendMailToAdd.Text.Trim();
		TypMail frM = default(TypMail);
		frM.From = eMa.Text;
		frM.Pass = ePass.Text;
		frM.Port = Conversions.ToInteger(ePort.Text);
		frM.Host = eHost.Text;
		frM.TimeOut = Conversions.ToInteger(eT.Text);
		frM.EnableSSL = eSSL.Checked;
		frM.UseDefaultCredentials = eUDC.Checked;
		sendingMail.FrM = frM;
		return sendingMail.SendMail(ini: false, toMail, "ПРРО WebCheck", "Тестування відправки чеків за допомогою ВебЧек: ПРРО");
	}

	private void SendTest_Click(object sender, EventArgs e)
	{
		//IL_00b9: Unknown result type (might be due to invalid IL or missing references)
		if (SendTestMail())
		{
			TypMail e2 = default(TypMail);
			e2.From = eMa.Text;
			e2.Pass = ePass.Text;
			e2.Host = eHost.Text;
			e2.TimeOut = Conversions.ToInteger(eT.Text);
			e2.Port = Conversions.ToInteger(ePort.Text);
			e2.EnableSSL = eSSL.Checked;
			e2.UseDefaultCredentials = eUDC.Checked;
			SMt.SaveEmail(e2);
			((Control)eSendEmail).Enabled = true;
		}
		else
		{
			Interaction.MsgBox((object)"Помилка надсилання листа!", (MsgBoxStyle)48, (object)"Налаштування SMTP");
		}
	}

	private void OkSave_Click(object sender, EventArgs e)
	{
		TypMail e2 = default(TypMail);
		e2.From = eMa.Text;
		e2.Pass = ePass.Text;
		e2.Host = eHost.Text;
		e2.TimeOut = Conversions.ToInteger(eT.Text);
		e2.Port = Conversions.ToInteger(ePort.Text);
		e2.EnableSSL = eSSL.Checked;
		e2.UseDefaultCredentials = eUDC.Checked;
		SMt.SaveEmail(e2);
	}

	private void eSendEmail_CheckedChanged(object sender, EventArgs e)
	{
		int val = (eSendEmail.Checked ? 1 : 0);
		All.f.IntigerWriteFN(All.A.FN, "SendToEmail", val);
	}
}
