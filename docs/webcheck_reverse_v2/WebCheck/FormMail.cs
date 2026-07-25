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
			EventHandler value2 = OkSave_Click;
			Button okSave = _OkSave;
			if (okSave != null)
			{
				okSave.Click -= value2;
			}
			_OkSave = value;
			okSave = _OkSave;
			if (okSave != null)
			{
				okSave.Click += value2;
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
			EventHandler value2 = SendTest_Click;
			Button sendTest = _SendTest;
			if (sendTest != null)
			{
				sendTest.Click -= value2;
			}
			_SendTest = value;
			sendTest = _SendTest;
			if (sendTest != null)
			{
				sendTest.Click += value2;
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
			EventHandler value2 = eSendEmail_CheckedChanged;
			CheckBox checkBox = _eSendEmail;
			if (checkBox != null)
			{
				checkBox.CheckedChanged -= value2;
			}
			_eSendEmail = value;
			checkBox = _eSendEmail;
			if (checkBox != null)
			{
				checkBox.CheckedChanged += value2;
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
		base.Load += FormMail_Load;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormMail));
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.Label5 = new System.Windows.Forms.Label();
		this.Label4 = new System.Windows.Forms.Label();
		this.Label3 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		this.Label2 = new System.Windows.Forms.Label();
		this.eUDC = new System.Windows.Forms.CheckBox();
		this.eSSL = new System.Windows.Forms.CheckBox();
		this.eT = new System.Windows.Forms.TextBox();
		this.ePort = new System.Windows.Forms.TextBox();
		this.eHost = new System.Windows.Forms.TextBox();
		this.ePass = new System.Windows.Forms.TextBox();
		this.eMa = new System.Windows.Forms.TextBox();
		this.OkSave = new System.Windows.Forms.Button();
		this.GroupBox2 = new System.Windows.Forms.GroupBox();
		this.Label6 = new System.Windows.Forms.Label();
		this.eSendEmail = new System.Windows.Forms.CheckBox();
		this.SendTest = new System.Windows.Forms.Button();
		this.SendMailToAdd = new System.Windows.Forms.TextBox();
		this.GroupBox1.SuspendLayout();
		this.GroupBox2.SuspendLayout();
		base.SuspendLayout();
		this.GroupBox1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.GroupBox1.Controls.Add(this.Label5);
		this.GroupBox1.Controls.Add(this.Label4);
		this.GroupBox1.Controls.Add(this.Label3);
		this.GroupBox1.Controls.Add(this.Label1);
		this.GroupBox1.Controls.Add(this.Label2);
		this.GroupBox1.Controls.Add(this.eUDC);
		this.GroupBox1.Controls.Add(this.eSSL);
		this.GroupBox1.Controls.Add(this.eT);
		this.GroupBox1.Controls.Add(this.ePort);
		this.GroupBox1.Controls.Add(this.eHost);
		this.GroupBox1.Controls.Add(this.ePass);
		this.GroupBox1.Controls.Add(this.eMa);
		this.GroupBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox1.Location = new System.Drawing.Point(12, 11);
		this.GroupBox1.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Padding = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox1.Size = new System.Drawing.Size(675, 272);
		this.GroupBox1.TabIndex = 6;
		this.GroupBox1.TabStop = false;
		this.GroupBox1.Text = "Налаштування";
		this.Label5.AutoSize = true;
		this.Label5.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label5.Location = new System.Drawing.Point(17, 214);
		this.Label5.Name = "Label5";
		this.Label5.Size = new System.Drawing.Size(85, 25);
		this.Label5.TabIndex = 11;
		this.Label5.Text = "Таймінг";
		this.Label4.AutoSize = true;
		this.Label4.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label4.Location = new System.Drawing.Point(17, 165);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(60, 25);
		this.Label4.TabIndex = 10;
		this.Label4.Text = "Порт";
		this.Label3.AutoSize = true;
		this.Label3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label3.Location = new System.Drawing.Point(17, 118);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(141, 25);
		this.Label3.TabIndex = 9;
		this.Label3.Text = "Сервер SMTP";
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(17, 74);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(80, 25);
		this.Label1.TabIndex = 8;
		this.Label1.Text = "Пароль";
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(17, 33);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(193, 25);
		this.Label2.TabIndex = 7;
		this.Label2.Text = "Електронна адреса";
		this.eUDC.AutoSize = true;
		this.eUDC.Location = new System.Drawing.Point(430, 210);
		this.eUDC.Name = "eUDC";
		this.eUDC.Size = new System.Drawing.Size(239, 29);
		this.eUDC.TabIndex = 6;
		this.eUDC.Text = "Use Default Credentials";
		this.eUDC.UseVisualStyleBackColor = true;
		this.eSSL.AutoSize = true;
		this.eSSL.Location = new System.Drawing.Point(430, 161);
		this.eSSL.Name = "eSSL";
		this.eSSL.Size = new System.Drawing.Size(239, 29);
		this.eSSL.TabIndex = 5;
		this.eSSL.Text = "Використовувати SSL";
		this.eSSL.UseVisualStyleBackColor = true;
		this.eT.Location = new System.Drawing.Point(230, 209);
		this.eT.Name = "eT";
		this.eT.Size = new System.Drawing.Size(170, 30);
		this.eT.TabIndex = 4;
		this.eT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ePort.Location = new System.Drawing.Point(230, 160);
		this.ePort.Name = "ePort";
		this.ePort.Size = new System.Drawing.Size(170, 30);
		this.ePort.TabIndex = 3;
		this.ePort.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.eHost.Location = new System.Drawing.Point(230, 113);
		this.eHost.Name = "eHost";
		this.eHost.Size = new System.Drawing.Size(342, 30);
		this.eHost.TabIndex = 2;
		this.eHost.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ePass.Location = new System.Drawing.Point(230, 69);
		this.ePass.Name = "ePass";
		this.ePass.Size = new System.Drawing.Size(342, 30);
		this.ePass.TabIndex = 1;
		this.ePass.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.eMa.Location = new System.Drawing.Point(230, 28);
		this.eMa.Name = "eMa";
		this.eMa.Size = new System.Drawing.Size(342, 30);
		this.eMa.TabIndex = 0;
		this.eMa.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OkSave.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.OkSave.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkSave.Location = new System.Drawing.Point(496, 38);
		this.OkSave.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.OkSave.Name = "OkSave";
		this.OkSave.Size = new System.Drawing.Size(164, 39);
		this.OkSave.TabIndex = 5;
		this.OkSave.Text = "Зберегти";
		this.OkSave.UseVisualStyleBackColor = true;
		this.OkSave.Visible = false;
		this.GroupBox2.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.GroupBox2.Controls.Add(this.Label6);
		this.GroupBox2.Controls.Add(this.eSendEmail);
		this.GroupBox2.Controls.Add(this.SendTest);
		this.GroupBox2.Controls.Add(this.OkSave);
		this.GroupBox2.Controls.Add(this.SendMailToAdd);
		this.GroupBox2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox2.Location = new System.Drawing.Point(12, 287);
		this.GroupBox2.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox2.Name = "GroupBox2";
		this.GroupBox2.Padding = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox2.Size = new System.Drawing.Size(675, 186);
		this.GroupBox2.TabIndex = 7;
		this.GroupBox2.TabStop = false;
		this.GroupBox2.Text = "Надсилати повідомлення";
		this.Label6.AutoSize = true;
		this.Label6.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label6.Location = new System.Drawing.Point(17, 45);
		this.Label6.Name = "Label6";
		this.Label6.Size = new System.Drawing.Size(374, 25);
		this.Label6.TabIndex = 10;
		this.Label6.Text = "Вкажіть електронну адресу для тесту:";
		this.eSendEmail.AutoSize = true;
		this.eSendEmail.Location = new System.Drawing.Point(22, 118);
		this.eSendEmail.Name = "eSendEmail";
		this.eSendEmail.Size = new System.Drawing.Size(295, 54);
		this.eSendEmail.TabIndex = 9;
		this.eSendEmail.Text = "відправляти повідомлення, \r\nякщо зазначений eMail";
		this.eSendEmail.UseVisualStyleBackColor = true;
		this.SendTest.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.SendTest.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SendTest.Location = new System.Drawing.Point(496, 103);
		this.SendTest.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.SendTest.Name = "SendTest";
		this.SendTest.Size = new System.Drawing.Size(164, 69);
		this.SendTest.TabIndex = 8;
		this.SendTest.Text = "Зберегти та відправити ";
		this.SendTest.UseVisualStyleBackColor = true;
		this.SendMailToAdd.Location = new System.Drawing.Point(22, 73);
		this.SendMailToAdd.Name = "SendMailToAdd";
		this.SendMailToAdd.Size = new System.Drawing.Size(369, 30);
		this.SendMailToAdd.TabIndex = 1;
		this.SendMailToAdd.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(699, 484);
		base.Controls.Add(this.GroupBox2);
		base.Controls.Add(this.GroupBox1);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormMail";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Налаштування SMTP";
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		this.GroupBox2.ResumeLayout(false);
		this.GroupBox2.PerformLayout();
		base.ResumeLayout(false);
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
			eSendEmail.Enabled = false;
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
			eSendEmail.Enabled = true;
		}
		else
		{
			Interaction.MsgBox("Помилка надсилання листа!", MsgBoxStyle.Exclamation, "Налаштування SMTP");
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
